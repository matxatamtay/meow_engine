use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

use meow_embedder_api::{BrowserEngine, CancellationToken};

#[tokio::test]
async fn http_document_reaches_interactive_display_list() {
    let mut body = String::from(
        "<!doctype html><title>alpha integration</title><style>p{height:20px;margin-bottom:4px}.hot{color:red}</style><div><h1>Visible by UA defaults</h1><p>basic document</p><a id='next' href='/next'>next</a></div><form action='/search'><input name='q'><input type='checkbox' name='safe' checked><button>Go</button></form><script>document.querySelector('#next').setAttribute('class','hot');document.title='scripted integration'</script>",
    );
    for index in 0..80 {
        body.push_str(&format!("<p>integration line {index}</p>"));
    }
    let url = serve_once(body).await;

    let mut engine = BrowserEngine::new();
    engine
        .navigate(&url, &CancellationToken::new())
        .await
        .expect("HTTP document should commit");

    let frame = engine
        .render_document_frame(320, 120)
        .expect("committed document should paint");
    assert!(!frame.display_list().is_empty());
    assert_eq!(engine.current_document().script_executions.len(), 1);
    assert!(engine.current_document().script_executions[0].succeeded());
    assert_eq!(
        engine.document_title(320, 120).unwrap(),
        "scripted integration · MeowEngine"
    );
    assert!(engine.hit_tests(320, 120).unwrap().entries().len() >= 4);
    assert!(engine.scroll_tree(320, 120).unwrap().content_height() > 120);
    assert!(engine.scroll_by(320, 120, 0, 48).unwrap());
    assert_eq!(engine.scroll_offset().y, 48);
}

async fn serve_once(body: String) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut request = [0_u8; 4096];
        let _ = stream.read(&mut request).await.unwrap();
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });
    format!("http://{address}/")
}

#[tokio::test]
async fn event_timers_forms_and_mutations_share_one_live_document_realm() {
    let body = String::from(
        "<!doctype html><title>interactive</title><style>button,a,input,p,form{display:block;margin-bottom:20px;height:28px}</style><button id='count'>Count</button><a id='stay' href='/gone'>Stay</a><p id='value'>0</p><form id='todo-form' action='/submit'><input id='todo' name='todo' required><button id='add'>Add</button></form><script>const count=document.querySelector('#count');const stay=document.querySelector('#stay');const value=document.querySelector('#value');const form=document.querySelector('#todo-form');const todo=document.querySelector('#todo');let clicks=0;count.addEventListener('click',()=>{value.textContent=String(++clicks);value.setAttribute('data-clicked','yes')});stay.addEventListener('click',event=>{event.preventDefault();value.textContent='blocked'});todo.addEventListener('invalid',()=>{value.textContent='invalid'});form.addEventListener('submit',event=>{event.preventDefault();value.textContent=todo.value;todo.value=''});setTimeout(()=>{value.textContent='timer';value.setAttribute('data-timer','yes');value.setAttribute('class','hot')},5)</script>",
    );
    let url = serve_once(body).await;
    let mut engine = BrowserEngine::new();
    engine
        .navigate(&url, &CancellationToken::new())
        .await
        .expect("interactive document should commit");
    engine.render_document_frame(640, 480).unwrap();
    let initial_builds = engine.mutation_pipeline_report().view_rebuilds;

    click_node(&mut engine, "count");
    assert_eq!(element_text(&engine, "value"), "1");
    let click_pipeline = engine.mutation_pipeline_report();
    assert!(click_pipeline.pending_records >= 2);
    assert!(click_pipeline.frame_scheduled);
    assert_eq!(click_pipeline.view_rebuilds, initial_builds);
    engine.render_document_frame(640, 480).unwrap();
    assert_eq!(
        engine.mutation_pipeline_report().view_rebuilds,
        initial_builds + 1
    );

    let result = click_node(&mut engine, "stay");
    assert!(result.navigation.is_none());
    assert_eq!(element_text(&engine, "value"), "blocked");
    engine.render_document_frame(640, 480).unwrap();

    let timer_builds = engine.mutation_pipeline_report().view_rebuilds;
    let timer = engine.advance_time(5, 8);
    assert_eq!(timer.tasks_run, 1);
    let timer_pipeline = engine.mutation_pipeline_report();
    assert!(timer_pipeline.pending_records >= 3);
    assert!(timer_pipeline.frame_scheduled);
    engine.render_document_frame(640, 480).unwrap();
    assert_eq!(
        engine.mutation_pipeline_report().view_rebuilds,
        timer_builds + 1
    );
    assert_eq!(element_text(&engine, "value"), "timer");

    let invalid = click_node(&mut engine, "add");
    assert!(invalid.navigation.is_none());
    assert_eq!(element_text(&engine, "value"), "invalid");
    engine.render_document_frame(640, 480).unwrap();

    let input = node_by_id(&engine, "todo");
    let point = point_for(&mut engine, input);
    engine.pointer_down(640, 480, point).unwrap();
    engine.pointer_up(640, 480, point).unwrap();
    engine
        .keyboard(
            640,
            480,
            meow_embedder_api::KeyboardCommand::Text("ship it".to_owned()),
        )
        .unwrap();
    engine.render_document_frame(640, 480).unwrap();
    let submitted = click_node(&mut engine, "add");
    assert!(submitted.navigation.is_none());
    assert_eq!(element_text(&engine, "value"), "ship it");
    let input_node = node_by_id(&engine, "todo");
    let input_element = engine
        .current_document()
        .document
        .element_by_id(input_node)
        .unwrap();
    assert_eq!(
        engine
            .current_document()
            .document
            .element_attribute(&input_element, "value")
            .as_deref(),
        Some("")
    );
    engine.render_document_frame(640, 480).unwrap();

    let input = node_by_id(&engine, "todo");
    let point = point_for(&mut engine, input);
    engine.pointer_down(640, 480, point).unwrap();
    engine.pointer_up(640, 480, point).unwrap();
    engine
        .keyboard(
            640,
            480,
            meow_embedder_api::KeyboardCommand::Text("next".to_owned()),
        )
        .unwrap();
    engine.render_document_frame(640, 480).unwrap();
    click_node(&mut engine, "add");
    assert_eq!(element_text(&engine, "value"), "next");
}

fn node_by_id(engine: &BrowserEngine, id: &str) -> meow_embedder_api::NodeId {
    engine
        .current_document()
        .document
        .elements_in_tree_order()
        .into_iter()
        .find(|element| {
            engine
                .current_document()
                .document
                .element_attribute(element, "id")
                .as_deref()
                == Some(id)
        })
        .expect("element id should exist")
        .id()
}

fn element_text(engine: &BrowserEngine, id: &str) -> String {
    let node = node_by_id(engine, id);
    let element = engine
        .current_document()
        .document
        .element_by_id(node)
        .expect("element should remain connected");
    engine.current_document().document.text_content(&element)
}

fn point_for(
    engine: &mut BrowserEngine,
    node: meow_embedder_api::NodeId,
) -> meow_embedder_api::InteractionPoint {
    let entry = engine
        .hit_tests(640, 480)
        .unwrap()
        .entries()
        .iter()
        .find(|entry| entry.node == node)
        .expect("interactive node should have hit geometry");
    meow_embedder_api::InteractionPoint::new(
        entry.rect.x.0 + entry.rect.width.0 / 2,
        entry.rect.y.0 + entry.rect.height.0 / 2,
    )
}

fn click_node(engine: &mut BrowserEngine, id: &str) -> meow_embedder_api::InteractionResult {
    let node = node_by_id(engine, id);
    let point = point_for(engine, node);
    engine.pointer_down(640, 480, point).unwrap();
    engine.pointer_up(640, 480, point).unwrap()
}
