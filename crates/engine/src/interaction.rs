//! W21-W23 scrolling, hit testing, focus, input controls, and default actions.

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
    time::Instant,
};

use meow_accessibility::focus_order as accessibility_focus_order;
use meow_display_list::{DisplayList, DisplayListError, Rgba8, Viewport};
use meow_html::{Document, NodeHandle, NodeId};
use meow_url_policy::BrowserUrl;

use crate::{
    BoxNode, ComputedStyleSnapshot, CssPx, DocumentState, FontDatabase, FragmentLayout,
    ImageResource, LayoutBox, LayoutRect, LayoutViewport, StyleSharingMetrics,
    build_box_tree_with_images, build_fragment_display_list_with_images_and_offset,
    fragments::append_bitmap_text, layout_fragment_tree, paint::fill_signed,
};

const CONTROL_BACKGROUND: Rgba8 = Rgba8::rgb(255, 255, 255);
const CONTROL_BORDER: Rgba8 = Rgba8::rgb(90, 96, 108);
const CONTROL_TEXT: Rgba8 = Rgba8::rgb(18, 23, 33);
const BUTTON_BACKGROUND: Rgba8 = Rgba8::rgb(232, 235, 240);
const FOCUS_RING: Rgba8 = Rgba8::rgb(40, 102, 220);
const CHECK_MARK: Rgba8 = Rgba8::rgb(32, 89, 190);

/// Integer point in viewport or document CSS pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InteractionPoint {
    pub x: i32,
    pub y: i32,
}

impl InteractionPoint {
    #[must_use]
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

/// Root document scroll offset.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ScrollOffset {
    pub x: i32,
    pub y: i32,
}

/// One scrollable layout node. W21 exposes nested overflow metadata even though
/// alpha input currently scrolls only the root viewport.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScrollNode {
    pub id: u32,
    pub source: Option<NodeId>,
    pub clip: LayoutRect,
    pub content_width: CssPx,
    pub content_height: CssPx,
}

/// Scroll metadata derived from the final layout tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScrollTree {
    viewport: Viewport,
    content_width: i32,
    content_height: i32,
    nodes: Vec<ScrollNode>,
}

impl ScrollTree {
    #[must_use]
    pub const fn viewport(&self) -> Viewport {
        self.viewport
    }

    #[must_use]
    pub const fn content_width(&self) -> i32 {
        self.content_width
    }

    #[must_use]
    pub const fn content_height(&self) -> i32 {
        self.content_height
    }

    #[must_use]
    pub fn nodes(&self) -> &[ScrollNode] {
        &self.nodes
    }

    #[must_use]
    pub fn maximum_offset(&self) -> ScrollOffset {
        ScrollOffset {
            x: (self.content_width - self.viewport.width as i32).max(0),
            y: (self.content_height - self.viewport.height as i32).max(0),
        }
    }

    #[must_use]
    pub fn clamp(&self, offset: ScrollOffset) -> ScrollOffset {
        let maximum = self.maximum_offset();
        ScrollOffset {
            x: offset.x.clamp(0, maximum.x),
            y: offset.y.clamp(0, maximum.y),
        }
    }
}

/// Public category returned by hit testing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HitTestKind {
    Link,
    TextInput,
    Checkbox,
    Button,
}

/// One document-space hit-test entry in paint order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HitTestEntry {
    pub node: NodeId,
    pub rect: LayoutRect,
    pub kind: HitTestKind,
    pub label: String,
}

/// Stable hit-test list generated from final layout and inline fragments.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HitTestList {
    entries: Vec<HitTestEntry>,
}

impl HitTestList {
    #[must_use]
    pub fn entries(&self) -> &[HitTestEntry] {
        &self.entries
    }

    #[must_use]
    pub fn hit_test(&self, point: InteractionPoint) -> Option<&HitTestEntry> {
        self.entries
            .iter()
            .rev()
            .find(|entry| contains(entry.rect, point))
    }
}

/// Result of one pointer or keyboard dispatch.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InteractionResult {
    pub redraw: bool,
    pub navigation: Option<BrowserUrl>,
    pub submitted_form: Option<NodeId>,
}

impl InteractionResult {
    fn redraw() -> Self {
        Self {
            redraw: true,
            navigation: None,
            submitted_form: None,
        }
    }

    fn navigate(url: BrowserUrl) -> Self {
        Self {
            redraw: true,
            navigation: Some(url),
            submitted_form: None,
        }
    }

    fn submit(form: NodeId, url: BrowserUrl) -> Self {
        Self {
            redraw: true,
            navigation: Some(url),
            submitted_form: Some(form),
        }
    }
}

/// Live form state mirrored back into DOM attributes before script events run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FormControlState {
    Text { node: NodeId, value: String },
    Checkbox { node: NodeId, checked: bool },
}

/// Backend-neutral keyboard command mapped by the embedder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeyboardCommand {
    Text(String),
    Tab { reverse: bool },
    Enter,
    Space,
    Backspace,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DocumentViewMetrics {
    pub style_micros: u64,
    pub box_tree_micros: u64,
    pub fragment_layout_micros: u64,
    pub interaction_micros: u64,
    pub total_build_micros: u64,
    pub style_elements: usize,
    pub style_sharing: StyleSharingMetrics,
    pub box_nodes: usize,
    pub layout_boxes: usize,
    pub paragraphs: usize,
    pub glyphs: usize,
    pub images: usize,
}

/// Cached layout, interaction geometry, and form metadata for one viewport.
#[derive(Clone, Debug)]
pub struct DocumentView {
    viewport: Viewport,
    styles: ComputedStyleSnapshot,
    fragments: FragmentLayout,
    images: BTreeMap<NodeId, Arc<ImageResource>>,
    scroll_tree: ScrollTree,
    hit_tests: HitTestList,
    targets: Vec<Target>,
    focus_chain: Vec<NodeId>,
    controls: Vec<Control>,
    forms: BTreeMap<NodeId, Form>,
    title: Option<String>,
    metrics: DocumentViewMetrics,
}

impl DocumentView {
    #[must_use]
    pub fn new(state: &DocumentState, viewport: Viewport) -> Self {
        let total_started = Instant::now();
        let document_span = tracing::info_span!(
            "document_view_build",
            width = viewport.width,
            height = viewport.height,
            url = %state.url,
        );
        let _document_guard = document_span.enter();

        let style_started = Instant::now();
        let styles = {
            let span = tracing::info_span!("style_compute");
            let _guard = span.enter();
            state.computed_styles()
        };
        let style_micros = elapsed_micros(style_started);

        let box_started = Instant::now();
        let boxes = {
            let span = tracing::info_span!("box_tree_build");
            let _guard = span.enter();
            build_box_tree_with_images(&state.document, &styles, &state.images)
        };
        let box_tree_micros = elapsed_micros(box_started);

        let layout_started = Instant::now();
        let mut fonts = FontDatabase::deterministic();
        let fragments = {
            let span = tracing::info_span!("fragment_layout");
            let _guard = span.enter();
            layout_fragment_tree(
                &boxes,
                &styles,
                LayoutViewport::new(viewport.width, viewport.height),
                &mut fonts,
            )
        };
        let fragment_layout_micros = elapsed_micros(layout_started);
        let style_elements = styles.elements().len();
        let style_sharing = styles.sharing_metrics();
        let box_nodes = boxes.roots().iter().map(count_box_nodes).sum();
        let layout_boxes = fragments
            .layout
            .roots()
            .iter()
            .map(count_layout_boxes)
            .sum();
        let paragraphs = fragments.fragments.paragraphs().len();
        let glyphs = fragments
            .fragments
            .paragraphs()
            .iter()
            .flat_map(|paragraph| &paragraph.lines)
            .map(|line| line.glyphs.len())
            .sum();

        let interaction_started = Instant::now();
        let source_bounds = collect_source_bounds(&fragments);
        let elements = state.document.elements_in_tree_order();
        let forms = collect_forms(state, &elements);
        let mut controls = Vec::new();
        let mut targets = Vec::new();
        let mut focus_chain = Vec::new();
        let mut title = None;

        for element in elements {
            let Some(local_name) = state.document.element_local_name(&element) else {
                continue;
            };
            if local_name.eq_ignore_ascii_case("title") && title.is_none() {
                let candidate = normalized_label(&state.document.text_content(&element));
                if !candidate.is_empty() {
                    title = Some(candidate);
                }
                continue;
            }
            if state
                .document
                .element_attribute(&element, "disabled")
                .is_some()
            {
                continue;
            }
            let node = element.id();
            let form = nearest_form(&state.document, &element);
            match local_name.as_str() {
                "a" => {
                    let Some(href) = state.document.element_attribute(&element, "href") else {
                        continue;
                    };
                    let Ok(url) = state.base_url.resolve(&href) else {
                        continue;
                    };
                    let label = normalized_label(&state.document.text_content(&element));
                    let rect = target_rect(
                        source_bounds.get(&node).copied(),
                        TargetGeometry::Link,
                        &label,
                    );
                    targets.push(Target {
                        node,
                        rect,
                        label,
                        kind: TargetKind::Link(url),
                    });
                    focus_chain.push(node);
                }
                "input" => {
                    let input_type = state
                        .document
                        .element_attribute(&element, "type")
                        .unwrap_or_else(|| "text".to_owned())
                        .to_ascii_lowercase();
                    let name = state.document.element_attribute(&element, "name");
                    let value = state
                        .document
                        .element_attribute(&element, "value")
                        .unwrap_or_default();
                    match input_type.as_str() {
                        "hidden" => controls.push(Control {
                            node,
                            form,
                            name,
                            initial: ControlInitial::Text(value),
                            successful: true,
                        }),
                        "checkbox" => {
                            controls.push(Control {
                                node,
                                form,
                                name: name.clone(),
                                initial: ControlInitial::Checkbox {
                                    checked: state
                                        .document
                                        .element_attribute(&element, "checked")
                                        .is_some(),
                                    value: if value.is_empty() {
                                        "on".to_owned()
                                    } else {
                                        value
                                    },
                                },
                                successful: true,
                            });
                            let rect = target_rect(
                                source_bounds.get(&node).copied(),
                                TargetGeometry::Checkbox,
                                "",
                            );
                            targets.push(Target {
                                node,
                                rect,
                                label: name.clone().unwrap_or_default(),
                                kind: TargetKind::Checkbox,
                            });
                            focus_chain.push(node);
                        }
                        "submit" | "button" => {
                            let label = if value.is_empty() {
                                if input_type == "submit" {
                                    "Submit".to_owned()
                                } else {
                                    "Button".to_owned()
                                }
                            } else {
                                value.clone()
                            };
                            let rect = target_rect(
                                source_bounds.get(&node).copied(),
                                TargetGeometry::Button,
                                &label,
                            );
                            targets.push(Target {
                                node,
                                rect,
                                label,
                                kind: TargetKind::Button {
                                    form,
                                    name,
                                    value,
                                    submits: input_type == "submit",
                                },
                            });
                            focus_chain.push(node);
                        }
                        "text" | "search" | "" => {
                            controls.push(Control {
                                node,
                                form,
                                name,
                                initial: ControlInitial::Text(value),
                                successful: true,
                            });
                            let rect = target_rect(
                                source_bounds.get(&node).copied(),
                                TargetGeometry::TextInput,
                                "",
                            );
                            targets.push(Target {
                                node,
                                rect,
                                label: String::new(),
                                kind: TargetKind::TextInput { form },
                            });
                            focus_chain.push(node);
                        }
                        _ => {}
                    }
                }
                "button" => {
                    let button_type = state
                        .document
                        .element_attribute(&element, "type")
                        .unwrap_or_else(|| "submit".to_owned())
                        .to_ascii_lowercase();
                    let mut label = normalized_label(&state.document.text_content(&element));
                    if label.is_empty() {
                        label = "Button".to_owned();
                    }
                    let value = state
                        .document
                        .element_attribute(&element, "value")
                        .unwrap_or_else(|| label.clone());
                    let name = state.document.element_attribute(&element, "name");
                    let rect = target_rect(
                        source_bounds.get(&node).copied(),
                        TargetGeometry::Button,
                        &label,
                    );
                    targets.push(Target {
                        node,
                        rect,
                        label,
                        kind: TargetKind::Button {
                            form,
                            name,
                            value,
                            submits: button_type != "button" && button_type != "reset",
                        },
                    });
                    focus_chain.push(node);
                }
                _ => {}
            }
        }

        let accessible_order = accessibility_focus_order(&state.document);
        focus_chain.sort_by_key(|node| {
            accessible_order
                .iter()
                .position(|candidate| candidate == node)
                .unwrap_or(usize::MAX)
        });
        focus_chain.retain(|node| accessible_order.contains(node));
        arrange_control_targets(&mut targets, viewport);
        let hit_tests = HitTestList {
            entries: targets
                .iter()
                .map(|target| HitTestEntry {
                    node: target.node,
                    rect: target.rect,
                    kind: target.kind.public_kind(),
                    label: target.label.clone(),
                })
                .collect(),
        };
        let scroll_tree = build_scroll_tree(&fragments, viewport);
        let interaction_micros = elapsed_micros(interaction_started);
        let metrics = DocumentViewMetrics {
            style_micros,
            box_tree_micros,
            fragment_layout_micros,
            interaction_micros,
            total_build_micros: elapsed_micros(total_started),
            style_elements,
            style_sharing,
            box_nodes,
            layout_boxes,
            paragraphs,
            glyphs,
            images: state.images.len(),
        };
        tracing::debug!(
            style_micros,
            box_tree_micros,
            fragment_layout_micros,
            interaction_micros,
            box_nodes,
            layout_boxes,
            glyphs,
            images = state.images.len(),
            "document view pipeline complete"
        );
        Self {
            viewport,
            styles,
            fragments,
            images: state.images.clone(),
            scroll_tree,
            hit_tests,
            targets,
            focus_chain,
            controls,
            forms,
            title,
            metrics,
        }
    }

    #[must_use]
    pub const fn viewport(&self) -> Viewport {
        self.viewport
    }

    #[must_use]
    pub const fn metrics(&self) -> DocumentViewMetrics {
        self.metrics
    }

    #[must_use]
    pub const fn scroll_tree(&self) -> &ScrollTree {
        &self.scroll_tree
    }

    #[must_use]
    pub const fn hit_tests(&self) -> &HitTestList {
        &self.hit_tests
    }

    #[must_use]
    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    pub fn display_list(
        &self,
        interaction: &InteractionState,
    ) -> Result<DisplayList, DisplayListError> {
        let span = tracing::info_span!(
            "display_list_build",
            width = self.viewport.width,
            height = self.viewport.height,
        );
        let _guard = span.enter();
        let scroll = self.scroll_tree.clamp(interaction.scroll);
        let mut list = build_fragment_display_list_with_images_and_offset(
            &self.fragments.layout,
            &self.styles,
            &self.fragments.fragments,
            self.viewport,
            &self.images,
            -scroll.x,
            -scroll.y,
        )?;
        for target in &self.targets {
            match &target.kind {
                TargetKind::TextInput { .. } => {
                    let rect = translate_for_scroll(target.rect, scroll);
                    paint_control_box(
                        &mut list,
                        rect,
                        interaction.focused == Some(target.node),
                        CONTROL_BACKGROUND,
                        self.viewport,
                    )?;
                    if let Some(ControlValue::Text(value)) = interaction.values.get(&target.node) {
                        let available = ((rect.width.0 - 8).max(0) / 6) as usize;
                        let visible = tail_characters(value, available);
                        append_bitmap_text(
                            &mut list,
                            &visible,
                            rect.x.0 + 4,
                            rect.y.0 + 15,
                            CONTROL_TEXT,
                            self.viewport,
                        )?;
                    }
                }
                TargetKind::Checkbox => {
                    let rect = translate_for_scroll(target.rect, scroll);
                    paint_control_box(
                        &mut list,
                        rect,
                        interaction.focused == Some(target.node),
                        CONTROL_BACKGROUND,
                        self.viewport,
                    )?;
                    if matches!(
                        interaction.values.get(&target.node),
                        Some(ControlValue::Checkbox { checked: true, .. })
                    ) {
                        append_bitmap_text(
                            &mut list,
                            "x",
                            rect.x.0 + 6,
                            rect.y.0 + 14,
                            CHECK_MARK,
                            self.viewport,
                        )?;
                    }
                }
                TargetKind::Button { .. } => {
                    let rect = translate_for_scroll(target.rect, scroll);
                    paint_control_box(
                        &mut list,
                        rect,
                        interaction.focused == Some(target.node),
                        BUTTON_BACKGROUND,
                        self.viewport,
                    )?;
                    let available = ((rect.width.0 - 12).max(0) / 6) as usize;
                    let label = first_characters(&target.label, available);
                    append_bitmap_text(
                        &mut list,
                        &label,
                        rect.x.0 + 6,
                        rect.y.0 + 15,
                        CONTROL_TEXT,
                        self.viewport,
                    )?;
                }
                TargetKind::Link(_) => {
                    if interaction.focused == Some(target.node) {
                        paint_outline(
                            &mut list,
                            translate_for_scroll(target.rect, scroll),
                            FOCUS_RING,
                            self.viewport,
                        )?;
                    }
                }
            }
        }
        Ok(list)
    }

    fn target(&self, node: NodeId) -> Option<&Target> {
        self.targets.iter().find(|target| target.node == node)
    }

    fn hit_target(&self, point: InteractionPoint, scroll: ScrollOffset) -> Option<&Target> {
        let document_point = InteractionPoint::new(point.x + scroll.x, point.y + scroll.y);
        let entry = self.hit_tests.hit_test(document_point)?;
        self.target(entry.node)
    }

    fn submission_url(
        &self,
        form: NodeId,
        interaction: &InteractionState,
        activated_button: Option<NodeId>,
    ) -> Option<BrowserUrl> {
        let descriptor = self.forms.get(&form)?;
        if !descriptor.get {
            return None;
        }
        let mut pairs = Vec::new();
        for control in &self.controls {
            if control.form != Some(form) || !control.successful {
                continue;
            }
            let Some(name) = control.name.as_ref().filter(|name| !name.is_empty()) else {
                continue;
            };
            match interaction.values.get(&control.node) {
                Some(ControlValue::Text(value)) => pairs.push((name.clone(), value.clone())),
                Some(ControlValue::Checkbox {
                    checked: true,
                    value,
                }) => pairs.push((name.clone(), value.clone())),
                Some(ControlValue::Checkbox { checked: false, .. }) | None => {}
            }
        }
        if let Some(button_node) = activated_button
            && let Some(Target {
                kind:
                    TargetKind::Button {
                        name: Some(name),
                        value,
                        ..
                    },
                ..
            }) = self.target(button_node)
            && !name.is_empty()
        {
            pairs.push((name.clone(), value.clone()));
        }
        Some(descriptor.action.with_query_pairs(&pairs))
    }
}

/// Mutable focus, pointer, scroll, and form-control state for one document.
#[derive(Clone, Debug, Default)]
pub struct InteractionState {
    scroll: ScrollOffset,
    focused: Option<NodeId>,
    pressed: Option<NodeId>,
    values: BTreeMap<NodeId, ControlValue>,
}

impl InteractionState {
    #[must_use]
    pub const fn scroll_offset(&self) -> ScrollOffset {
        self.scroll
    }

    #[must_use]
    pub const fn focused_node(&self) -> Option<NodeId> {
        self.focused
    }

    #[must_use]
    pub fn text_value(&self, node: NodeId) -> Option<&str> {
        match self.values.get(&node) {
            Some(ControlValue::Text(value)) => Some(value),
            _ => None,
        }
    }

    #[must_use]
    pub fn checkbox_checked(&self, node: NodeId) -> Option<bool> {
        match self.values.get(&node) {
            Some(ControlValue::Checkbox { checked, .. }) => Some(*checked),
            _ => None,
        }
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn reconcile(&mut self, view: &DocumentView) {
        let mut valid = BTreeSet::new();
        for control in &view.controls {
            valid.insert(control.node);
            let value = match &control.initial {
                ControlInitial::Text(value) => ControlValue::Text(value.clone()),
                ControlInitial::Checkbox { checked, value } => ControlValue::Checkbox {
                    checked: *checked,
                    value: value.clone(),
                },
            };
            self.values.insert(control.node, value);
        }
        self.values.retain(|node, _| valid.contains(node));
        if self
            .focused
            .is_some_and(|node| !view.focus_chain.contains(&node))
        {
            self.focused = None;
        }
        if self.pressed.is_some_and(|node| view.target(node).is_none()) {
            self.pressed = None;
        }
        self.scroll = view.scroll_tree.clamp(self.scroll);
    }

    pub fn scroll_by(&mut self, view: &DocumentView, delta_x: i32, delta_y: i32) -> bool {
        let before = self.scroll;
        self.scroll = view.scroll_tree.clamp(ScrollOffset {
            x: self.scroll.x.saturating_add(delta_x),
            y: self.scroll.y.saturating_add(delta_y),
        });
        self.scroll != before
    }

    pub fn pointer_down(
        &mut self,
        view: &DocumentView,
        viewport_point: InteractionPoint,
    ) -> InteractionResult {
        let target = view.hit_target(viewport_point, self.scroll);
        let previous_focus = self.focused;
        self.pressed = target.map(|target| target.node);
        self.focused = target.map(|target| target.node);
        InteractionResult {
            redraw: previous_focus != self.focused,
            navigation: None,
            submitted_form: None,
        }
    }

    #[must_use]
    pub fn click_target(
        &self,
        view: &DocumentView,
        viewport_point: InteractionPoint,
    ) -> Option<NodeId> {
        let released = view
            .hit_target(viewport_point, self.scroll)
            .map(|target| target.node);
        self.pressed.filter(|pressed| Some(*pressed) == released)
    }

    pub fn pointer_up(
        &mut self,
        view: &DocumentView,
        viewport_point: InteractionPoint,
    ) -> InteractionResult {
        self.pointer_up_with_default(view, viewport_point, true)
    }

    pub fn pointer_up_with_default(
        &mut self,
        view: &DocumentView,
        viewport_point: InteractionPoint,
        allow_default: bool,
    ) -> InteractionResult {
        let target = self.click_target(view, viewport_point);
        self.pressed = None;
        if allow_default {
            target.map_or_else(InteractionResult::default, |node| self.activate(view, node))
        } else {
            InteractionResult::default()
        }
    }

    #[must_use]
    pub fn keyboard_click_target(
        &self,
        view: &DocumentView,
        command: &KeyboardCommand,
    ) -> Option<NodeId> {
        let focused = self.focused?;
        let target = view.target(focused)?;
        match (&target.kind, command) {
            (TargetKind::Link(_), KeyboardCommand::Enter)
            | (TargetKind::Button { .. }, KeyboardCommand::Enter)
            | (TargetKind::Button { .. }, KeyboardCommand::Space)
            | (TargetKind::Checkbox, KeyboardCommand::Enter)
            | (TargetKind::Checkbox, KeyboardCommand::Space) => Some(focused),
            _ => None,
        }
    }

    pub fn keyboard(&mut self, view: &DocumentView, command: KeyboardCommand) -> InteractionResult {
        self.keyboard_with_default(view, command, true)
    }

    pub fn keyboard_with_default(
        &mut self,
        view: &DocumentView,
        command: KeyboardCommand,
        allow_default: bool,
    ) -> InteractionResult {
        if let KeyboardCommand::Tab { reverse } = command {
            return InteractionResult {
                redraw: self.focus_next(view, reverse),
                navigation: None,
                submitted_form: None,
            };
        }
        let Some(focused) = self.focused else {
            return InteractionResult::default();
        };
        let Some(target) = view.target(focused) else {
            return InteractionResult::default();
        };
        match (&target.kind, command) {
            (TargetKind::TextInput { .. }, KeyboardCommand::Text(text)) => {
                if let Some(ControlValue::Text(value)) = self.values.get_mut(&focused) {
                    value.extend(text.chars().filter(|character| !character.is_control()));
                    InteractionResult::redraw()
                } else {
                    InteractionResult::default()
                }
            }
            (TargetKind::TextInput { .. }, KeyboardCommand::Backspace) => {
                if let Some(ControlValue::Text(value)) = self.values.get_mut(&focused) {
                    let changed = value.pop().is_some();
                    InteractionResult {
                        redraw: changed,
                        navigation: None,
                        submitted_form: None,
                    }
                } else {
                    InteractionResult::default()
                }
            }
            (TargetKind::TextInput { form: Some(form) }, KeyboardCommand::Enter) => view
                .submission_url(*form, self, None)
                .map_or_else(InteractionResult::default, |url| {
                    InteractionResult::submit(*form, url)
                }),
            (TargetKind::Link(_), KeyboardCommand::Enter)
            | (TargetKind::Button { .. }, KeyboardCommand::Enter)
            | (TargetKind::Button { .. }, KeyboardCommand::Space)
            | (TargetKind::Checkbox, KeyboardCommand::Enter)
            | (TargetKind::Checkbox, KeyboardCommand::Space)
                if allow_default =>
            {
                self.activate(view, focused)
            }
            (TargetKind::Link(_), KeyboardCommand::Enter)
            | (TargetKind::Button { .. }, KeyboardCommand::Enter)
            | (TargetKind::Button { .. }, KeyboardCommand::Space)
            | (TargetKind::Checkbox, KeyboardCommand::Enter)
            | (TargetKind::Checkbox, KeyboardCommand::Space) => InteractionResult::default(),
            _ => InteractionResult::default(),
        }
    }

    #[must_use]
    pub fn control_states(&self) -> Vec<FormControlState> {
        self.values
            .iter()
            .map(|(node, value)| match value {
                ControlValue::Text(value) => FormControlState::Text {
                    node: *node,
                    value: value.clone(),
                },
                ControlValue::Checkbox { checked, .. } => FormControlState::Checkbox {
                    node: *node,
                    checked: *checked,
                },
            })
            .collect()
    }

    fn focus_next(&mut self, view: &DocumentView, reverse: bool) -> bool {
        if view.focus_chain.is_empty() {
            return false;
        }
        let current = self
            .focused
            .and_then(|focused| view.focus_chain.iter().position(|node| *node == focused));
        let next = match (current, reverse) {
            (Some(index), false) => (index + 1) % view.focus_chain.len(),
            (Some(0), true) | (None, true) => view.focus_chain.len() - 1,
            (Some(index), true) => index - 1,
            (None, false) => 0,
        };
        let changed = self.focused != Some(view.focus_chain[next]);
        self.focused = Some(view.focus_chain[next]);
        changed
    }

    fn activate(&mut self, view: &DocumentView, node: NodeId) -> InteractionResult {
        let Some(target) = view.target(node) else {
            return InteractionResult::default();
        };
        match &target.kind {
            TargetKind::Link(url) => InteractionResult::navigate(url.clone()),
            TargetKind::TextInput { .. } => InteractionResult::default(),
            TargetKind::Checkbox => {
                if let Some(ControlValue::Checkbox { checked, .. }) = self.values.get_mut(&node) {
                    *checked = !*checked;
                    InteractionResult::redraw()
                } else {
                    InteractionResult::default()
                }
            }
            TargetKind::Button {
                form: Some(form),
                submits: true,
                ..
            } => view
                .submission_url(*form, self, Some(node))
                .map_or_else(InteractionResult::default, |url| {
                    InteractionResult::submit(*form, url)
                }),
            TargetKind::Button { .. } => InteractionResult::redraw(),
        }
    }
}

#[derive(Clone, Debug)]
struct Form {
    action: BrowserUrl,
    get: bool,
}

#[derive(Clone, Debug)]
struct Control {
    node: NodeId,
    form: Option<NodeId>,
    name: Option<String>,
    initial: ControlInitial,
    successful: bool,
}

#[derive(Clone, Debug)]
enum ControlInitial {
    Text(String),
    Checkbox { checked: bool, value: String },
}

#[derive(Clone, Debug)]
enum ControlValue {
    Text(String),
    Checkbox { checked: bool, value: String },
}

#[derive(Clone, Debug)]
struct Target {
    node: NodeId,
    rect: LayoutRect,
    label: String,
    kind: TargetKind,
}

#[derive(Clone, Debug)]
enum TargetKind {
    Link(BrowserUrl),
    TextInput {
        form: Option<NodeId>,
    },
    Checkbox,
    Button {
        form: Option<NodeId>,
        name: Option<String>,
        value: String,
        submits: bool,
    },
}

impl TargetKind {
    const fn public_kind(&self) -> HitTestKind {
        match self {
            Self::Link(_) => HitTestKind::Link,
            Self::TextInput { .. } => HitTestKind::TextInput,
            Self::Checkbox => HitTestKind::Checkbox,
            Self::Button { .. } => HitTestKind::Button,
        }
    }
}

#[derive(Clone, Copy)]
enum TargetGeometry {
    Link,
    TextInput,
    Checkbox,
    Button,
}

fn collect_forms(state: &DocumentState, elements: &[NodeHandle]) -> BTreeMap<NodeId, Form> {
    let mut forms = BTreeMap::new();
    for element in elements {
        if state.document.element_local_name(element).as_deref() != Some("form") {
            continue;
        }
        let action = state
            .document
            .element_attribute(element, "action")
            .and_then(|action| state.base_url.resolve(&action).ok())
            .unwrap_or_else(|| state.url.clone());
        let method = state
            .document
            .element_attribute(element, "method")
            .unwrap_or_else(|| "get".to_owned());
        forms.insert(
            element.id(),
            Form {
                action,
                get: method.eq_ignore_ascii_case("get"),
            },
        );
    }
    forms
}

fn nearest_form(document: &Document, element: &NodeHandle) -> Option<NodeId> {
    let mut current = document.parent_element(element);
    while let Some(candidate) = current {
        if document.element_local_name(&candidate).as_deref() == Some("form") {
            return Some(candidate.id());
        }
        current = document.parent_element(&candidate);
    }
    None
}

fn elapsed_micros(started: Instant) -> u64 {
    started.elapsed().as_micros().min(u128::from(u64::MAX)) as u64
}

fn count_box_nodes(node: &BoxNode) -> usize {
    1 + node.children.iter().map(count_box_nodes).sum::<usize>()
}

fn count_layout_boxes(node: &LayoutBox) -> usize {
    1 + node.children.iter().map(count_layout_boxes).sum::<usize>()
}

fn collect_source_bounds(layout: &FragmentLayout) -> BTreeMap<NodeId, LayoutRect> {
    let mut bounds = BTreeMap::new();
    for root in layout.layout.roots() {
        collect_layout_bounds(root, &mut bounds);
    }
    for paragraph in layout.fragments.paragraphs() {
        if let Some(source) = paragraph.source {
            union_into(&mut bounds, source, paragraph.rect);
        }
        for line in &paragraph.lines {
            for glyph in &line.glyphs {
                if let Some(source) = glyph.source {
                    union_into(
                        &mut bounds,
                        source,
                        LayoutRect {
                            x: glyph.x,
                            y: CssPx(glyph.baseline.0 - 12),
                            width: CssPx(glyph.advance.0.max(1)),
                            height: CssPx(16),
                        },
                    );
                }
            }
        }
    }
    bounds
}

fn collect_layout_bounds(node: &LayoutBox, bounds: &mut BTreeMap<NodeId, LayoutRect>) {
    if let Some(source) = node.source {
        union_into(bounds, source, node.border_box_rect());
    }
    for child in &node.children {
        collect_layout_bounds(child, bounds);
    }
}

fn union_into(bounds: &mut BTreeMap<NodeId, LayoutRect>, source: NodeId, rect: LayoutRect) {
    bounds
        .entry(source)
        .and_modify(|existing| *existing = union(*existing, rect))
        .or_insert(rect);
}

fn union(left: LayoutRect, right: LayoutRect) -> LayoutRect {
    let x0 = left.x.0.min(right.x.0);
    let y0 = left.y.0.min(right.y.0);
    let x1 = (left.x.0 + left.width.0).max(right.x.0 + right.width.0);
    let y1 = (left.y.0 + left.height.0).max(right.y.0 + right.height.0);
    LayoutRect {
        x: CssPx(x0),
        y: CssPx(y0),
        width: CssPx((x1 - x0).max(0)),
        height: CssPx((y1 - y0).max(0)),
    }
}

fn arrange_control_targets(targets: &mut [Target], viewport: Viewport) {
    let mut cursor_x: i32 = 8;
    let mut cursor_y: i32 = 8;
    let mut row_height: i32 = 0;
    let maximum_x = viewport.width as i32 - 8;
    for target in targets {
        if matches!(target.kind, TargetKind::Link(_)) {
            continue;
        }
        let preferred_y = target.rect.y.0.max(8);
        if preferred_y > cursor_y.saturating_add(row_height).saturating_add(8) {
            cursor_x = 8;
            cursor_y = preferred_y;
            row_height = 0;
        }
        if cursor_x.saturating_add(target.rect.width.0) > maximum_x && cursor_x > 8 {
            cursor_x = 8;
            cursor_y = cursor_y.saturating_add(row_height).saturating_add(8);
            row_height = 0;
        }
        target.rect.x = CssPx(cursor_x);
        target.rect.y = CssPx(cursor_y);
        cursor_x = cursor_x
            .saturating_add(target.rect.width.0)
            .saturating_add(8);
        row_height = row_height.max(target.rect.height.0);
    }
}

fn target_rect(rect: Option<LayoutRect>, geometry: TargetGeometry, label: &str) -> LayoutRect {
    let mut rect = rect.unwrap_or_default();
    match geometry {
        TargetGeometry::Link => {
            rect.width = CssPx(rect.width.0.max(1));
            rect.height = CssPx(rect.height.0.max(16));
        }
        TargetGeometry::TextInput => {
            rect.width = CssPx(rect.width.0.max(160));
            rect.height = CssPx(rect.height.0.max(22));
        }
        TargetGeometry::Checkbox => {
            rect.width = CssPx(18);
            rect.height = CssPx(18);
        }
        TargetGeometry::Button => {
            let intrinsic = i32::try_from(label.chars().count())
                .unwrap_or(i32::MAX)
                .saturating_mul(6)
                .saturating_add(16);
            rect.width = CssPx(rect.width.0.max(intrinsic.max(64)));
            rect.height = CssPx(rect.height.0.max(22));
        }
    }
    rect
}

fn build_scroll_tree(layout: &FragmentLayout, viewport: Viewport) -> ScrollTree {
    let mut content_width = viewport.width as i32;
    let mut content_height = viewport.height as i32;
    let mut nested = Vec::new();
    let mut next_id = 1;
    for root in layout.layout.roots() {
        collect_scroll_nodes(
            root,
            &mut content_width,
            &mut content_height,
            &mut next_id,
            &mut nested,
        );
    }
    for paragraph in layout.fragments.paragraphs() {
        content_width = content_width.max(paragraph.rect.x.0 + paragraph.rect.width.0);
        content_height = content_height.max(paragraph.rect.y.0 + paragraph.rect.height.0);
    }
    let mut nodes = vec![ScrollNode {
        id: 0,
        source: None,
        clip: LayoutRect {
            x: CssPx(0),
            y: CssPx(0),
            width: CssPx(viewport.width as i32),
            height: CssPx(viewport.height as i32),
        },
        content_width: CssPx(content_width.max(0)),
        content_height: CssPx(content_height.max(0)),
    }];
    nodes.extend(nested);
    ScrollTree {
        viewport,
        content_width: content_width.max(0),
        content_height: content_height.max(0),
        nodes,
    }
}

fn collect_scroll_nodes(
    node: &LayoutBox,
    content_width: &mut i32,
    content_height: &mut i32,
    next_id: &mut u32,
    output: &mut Vec<ScrollNode>,
) {
    let rect = node.border_box_rect();
    *content_width = (*content_width).max(rect.x.0 + rect.width.0);
    *content_height = (*content_height).max(rect.y.0 + rect.height.0);
    if node.overflow.horizontal || node.overflow.vertical {
        output.push(ScrollNode {
            id: *next_id,
            source: node.source,
            clip: node.content,
            content_width: node.overflow.scroll_width,
            content_height: node.overflow.scroll_height,
        });
        *next_id = (*next_id).saturating_add(1);
    }
    for child in &node.children {
        collect_scroll_nodes(child, content_width, content_height, next_id, output);
    }
}

fn contains(rect: LayoutRect, point: InteractionPoint) -> bool {
    point.x >= rect.x.0
        && point.y >= rect.y.0
        && point.x < rect.x.0.saturating_add(rect.width.0)
        && point.y < rect.y.0.saturating_add(rect.height.0)
}

fn translate_for_scroll(rect: LayoutRect, scroll: ScrollOffset) -> LayoutRect {
    LayoutRect {
        x: CssPx(rect.x.0 - scroll.x),
        y: CssPx(rect.y.0 - scroll.y),
        ..rect
    }
}

fn paint_control_box(
    list: &mut DisplayList,
    rect: LayoutRect,
    focused: bool,
    background: Rgba8,
    viewport: Viewport,
) -> Result<(), DisplayListError> {
    fill_signed(list, rect, background, viewport)?;
    paint_outline(list, rect, CONTROL_BORDER, viewport)?;
    if focused {
        paint_outline(
            list,
            LayoutRect {
                x: CssPx(rect.x.0 - 2),
                y: CssPx(rect.y.0 - 2),
                width: CssPx(rect.width.0 + 4),
                height: CssPx(rect.height.0 + 4),
            },
            FOCUS_RING,
            viewport,
        )?;
    }
    Ok(())
}

fn paint_outline(
    list: &mut DisplayList,
    rect: LayoutRect,
    color: Rgba8,
    viewport: Viewport,
) -> Result<(), DisplayListError> {
    fill_signed(
        list,
        LayoutRect {
            height: CssPx(1),
            ..rect
        },
        color,
        viewport,
    )?;
    fill_signed(
        list,
        LayoutRect {
            y: CssPx(rect.y.0 + rect.height.0 - 1),
            height: CssPx(1),
            ..rect
        },
        color,
        viewport,
    )?;
    fill_signed(
        list,
        LayoutRect {
            width: CssPx(1),
            ..rect
        },
        color,
        viewport,
    )?;
    fill_signed(
        list,
        LayoutRect {
            x: CssPx(rect.x.0 + rect.width.0 - 1),
            width: CssPx(1),
            ..rect
        },
        color,
        viewport,
    )
}

fn normalized_label(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn first_characters(value: &str, limit: usize) -> String {
    value.chars().take(limit).collect()
}

fn tail_characters(value: &str, limit: usize) -> String {
    let count = value.chars().count();
    value.chars().skip(count.saturating_sub(limit)).collect()
}

#[cfg(test)]
mod tests {
    use meow_css::parse_stylesheet;
    use meow_html::parse_utf8;

    use super::*;
    use crate::{CharsetSource, DocumentStylesheet, StylesheetSource};

    fn document_state(html: &[u8], css: &str) -> DocumentState {
        let url = BrowserUrl::parse("https://example.test/docs/index.html").unwrap();
        DocumentState {
            url: url.clone(),
            base_url: url,
            document: parse_utf8(html).document,
            encoding: "UTF-8",
            charset_source: CharsetSource::Default,
            response: None,
            stylesheets: vec![DocumentStylesheet {
                source: StylesheetSource::Inline {
                    node: NodeId {
                        document: 0,
                        slot: 0,
                        generation: 0,
                    },
                },
                media: None,
                stylesheet: parse_stylesheet(css),
            }],
            stylesheet_errors: Vec::new(),
            script_executions: Vec::new(),
            script_mutations: Vec::new(),
            images: Default::default(),
            image_errors: Vec::new(),
            image_cache_metrics: Default::default(),
            history_index: 0,
        }
    }

    #[test]
    fn scroll_is_clamped_and_hit_testing_uses_document_coordinates() {
        let mut html = String::from("<main><a href='/next'>top</a>");
        for index in 0..80 {
            html.push_str(&format!("<p>line {index}</p>"));
        }
        html.push_str("</main>");
        let state = document_state(html.as_bytes(), "p { display:block; height:20px }");
        let view = DocumentView::new(&state, Viewport::new(320, 120).unwrap());
        let mut interaction = InteractionState::default();
        interaction.reconcile(&view);

        assert!(interaction.scroll_by(&view, 0, 10_000));
        assert_eq!(
            interaction.scroll_offset().y,
            view.scroll_tree().maximum_offset().y
        );
        assert!(!view.hit_tests().entries().is_empty());
    }

    #[test]
    fn tab_typing_checkbox_and_get_submission_share_live_state() {
        let state = document_state(
            br#"<form action='/search'><input name='q'><input type='checkbox' name='safe' checked><button name='go' value='yes'>Find</button></form>"#,
            "form { display:block } input, button { display:inline }",
        );
        let view = DocumentView::new(&state, Viewport::new(400, 160).unwrap());
        let mut interaction = InteractionState::default();
        interaction.reconcile(&view);

        assert!(
            interaction
                .keyboard(&view, KeyboardCommand::Tab { reverse: false })
                .redraw
        );
        interaction.keyboard(&view, KeyboardCommand::Text("cats".to_owned()));
        interaction.keyboard(&view, KeyboardCommand::Tab { reverse: false });
        interaction.keyboard(&view, KeyboardCommand::Space);
        interaction.keyboard(&view, KeyboardCommand::Tab { reverse: false });
        let result = interaction.keyboard(&view, KeyboardCommand::Enter);
        let url = result.navigation.expect("submit should navigate");
        assert_eq!(url.as_str(), "https://example.test/search?q=cats&go=yes");
    }
}
