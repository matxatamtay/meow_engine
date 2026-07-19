(() => {
    'use strict';
    const cache = new Map();
    const listeners = new Map();
    const timerCallbacks = new Map();
    const fetchPromises = new Map();
    const webSockets = new Map();
    const customTargets = new Map();
    let nextAbortSignalId = 1;

    function listenerOptions(options) {
        if (options === true || options === false) {
            return { capture: Boolean(options), once: false };
        }
        return {
            capture: Boolean(options && options.capture),
            once: Boolean(options && options.once),
        };
    }

    function targetKey(target) {
        if (target === globalThis) return '@window';
        if (target === document) return '@document';
        return target && (target.__meowTargetKey ?? target.__meowId);
    }

    function targetForKey(key) {
        if (key === '@window') return globalThis;
        if (key === '@document') return document;
        if (typeof key === 'string' && key.startsWith('@')) return customTargets.get(key) ?? null;
        return wrap(key);
    }

    class Event {
        constructor(type, init = {}) {
            if (arguments.length === 0) throw new TypeError('Event type is required');
            this.type = String(type);
            this.bubbles = Boolean(init.bubbles);
            this.cancelable = Boolean(init.cancelable);
            this.defaultPrevented = false;
            this.eventPhase = Event.NONE;
            this.target = null;
            this.currentTarget = null;
            this.__stopped = false;
            this.__immediateStopped = false;
        }
        preventDefault() {
            if (this.cancelable) this.defaultPrevented = true;
        }
        stopPropagation() { this.__stopped = true; }
        stopImmediatePropagation() {
            this.__stopped = true;
            this.__immediateStopped = true;
        }
    }
    Object.defineProperties(Event, {
        NONE: { value: 0 },
        CAPTURING_PHASE: { value: 1 },
        AT_TARGET: { value: 2 },
        BUBBLING_PHASE: { value: 3 },
    });
    Object.defineProperties(Event.prototype, {
        NONE: { value: 0 },
        CAPTURING_PHASE: { value: 1 },
        AT_TARGET: { value: 2 },
        BUBBLING_PHASE: { value: 3 },
    });

    function invokeListeners(target, event, capture, phase) {
        const key = targetKey(target);
        const byType = listeners.get(key);
        const bucket = byType && byType.get(event.type);
        if (!bucket) return;
        event.currentTarget = target;
        event.eventPhase = phase;
        for (const listener of [...bucket]) {
            if (listener.capture !== capture) continue;
            if (listener.once) {
                const index = bucket.indexOf(listener);
                if (index >= 0) bucket.splice(index, 1);
            }
            const callback = listener.callback;
            if (typeof callback === 'function') {
                callback.call(target, event);
            } else if (callback && typeof callback.handleEvent === 'function') {
                callback.handleEvent(event);
            }
            if (event.__immediateStopped) break;
        }
    }

    function dispatch(target, event) {
        if (!(event instanceof Event)) throw new TypeError('dispatchEvent expects an Event');
        if (event.eventPhase !== Event.NONE) throw new Error('event is already being dispatched');
        const key = targetKey(target);
        let pathKeys;
        if (key === '@window') pathKeys = ['@window'];
        else if (key === '@document') pathKeys = ['@document', '@window'];
        else if (typeof key === 'string' && key.startsWith('@')) pathKeys = [key];
        else pathKeys = __meow_event_path(key).split(',').filter(Boolean);
        const path = pathKeys.map(targetForKey);
        event.target = target;

        for (let index = path.length - 1; index >= 1 && !event.__stopped; index--) {
            invokeListeners(path[index], event, true, Event.CAPTURING_PHASE);
        }
        if (!event.__stopped) {
            invokeListeners(path[0], event, true, Event.AT_TARGET);
            if (!event.__immediateStopped) {
                invokeListeners(path[0], event, false, Event.AT_TARGET);
            }
        }
        if (event.bubbles && !event.__stopped) {
            for (let index = 1; index < path.length && !event.__stopped; index++) {
                invokeListeners(path[index], event, false, Event.BUBBLING_PHASE);
            }
        }
        event.currentTarget = null;
        event.eventPhase = Event.NONE;
        return !event.defaultPrevented;
    }

    class EventTarget {
        addEventListener(type, callback, options = false) {
            if (callback === null || callback === undefined) return;
            type = String(type);
            const normalized = listenerOptions(options);
            const key = targetKey(this);
            let byType = listeners.get(key);
            if (!byType) listeners.set(key, byType = new Map());
            let bucket = byType.get(type);
            if (!bucket) byType.set(type, bucket = []);
            if (bucket.some(item => item.callback === callback && item.capture === normalized.capture)) return;
            bucket.push({ callback, capture: normalized.capture, once: normalized.once });
        }
        removeEventListener(type, callback, options = false) {
            const key = targetKey(this);
            const bucket = listeners.get(key)?.get(String(type));
            if (!bucket) return;
            const capture = listenerOptions(options).capture;
            const index = bucket.findIndex(item => item.callback === callback && item.capture === capture);
            if (index >= 0) bucket.splice(index, 1);
        }
        dispatchEvent(event) { return dispatch(this, event); }
    }

    class Node extends EventTarget {
        constructor(id) {
            super();
            Object.defineProperty(this, '__meowId', { value: id });
        }
        get textContent() {
            return __meow_text_content(this.__meowId);
        }
        set textContent(value) {
            __meow_set_text_content(this.__meowId, String(value));
        }
        get parentElement() {
            return wrap(__meow_parent_element(this.__meowId));
        }
    }

    class Element extends Node {
        get localName() {
            return __meow_local_name(this.__meowId);
        }
        get firstElementChild() {
            return wrap(__meow_first_element_child(this.__meowId));
        }
        get nextElementSibling() {
            return wrap(__meow_next_element_sibling(this.__meowId));
        }
        get value() { return __meow_form_value(this.__meowId); }
        set value(value) { __meow_set_form_value(this.__meowId, String(value)); }
        get checked() { return __meow_form_checked(this.__meowId); }
        set checked(value) { __meow_set_form_checked(this.__meowId, Boolean(value)); }
        getAttribute(name) {
            return __meow_get_attribute(this.__meowId, String(name));
        }
        setAttribute(name, value) {
            __meow_set_attribute(this.__meowId, String(name), String(value));
        }
        removeAttribute(name) {
            __meow_remove_attribute(this.__meowId, String(name));
        }
        querySelector(selector) {
            return wrap(__meow_query_selector(this.__meowId, String(selector)));
        }
    }

    class Document extends Node {
        constructor() { super(null); }
        get textContent() { return null; }
        set textContent(_) {}
        get title() { return __meow_document_title(); }
        set title(value) { __meow_set_document_title(String(value)); }
        get documentElement() { return wrap(__meow_query_selector(null, 'html')); }
        querySelector(selector) {
            return wrap(__meow_query_selector(null, String(selector)));
        }
    }

    class Location {
        get href() { return __meow_location(); }
        toString() { return this.href; }
    }

    function wrap(id) {
        if (id === null || id === undefined) return null;
        let value = cache.get(id);
        if (value === undefined) {
            value = new Element(id);
            cache.set(id, value);
        }
        return value;
    }

    function normalizeDelay(value) {
        const number = Number(value);
        if (!Number.isFinite(number) || number <= 0) return 0;
        return Math.floor(number);
    }

    function scheduleTimer(repeat, callback, delay, args) {
        if (typeof callback !== 'function') {
            const source = String(callback);
            callback = () => Function(source)();
        }
        const id = __meow_schedule_timer(normalizeDelay(delay), repeat);
        timerCallbacks.set(id, { callback, args, repeat });
        return id;
    }

    function setTimeout(callback, delay = 0, ...args) {
        return scheduleTimer(false, callback, delay, args);
    }
    function setInterval(callback, delay = 0, ...args) {
        return scheduleTimer(true, callback, delay, args);
    }
    function clearTimer(id) {
        id = Number(id);
        timerCallbacks.delete(id);
        __meow_cancel_timer(id);
    }
    function queueMicrotask(callback) {
        if (typeof callback !== 'function') throw new TypeError('queueMicrotask expects a function');
        Promise.resolve().then(callback);
    }

    function formatConsoleValue(value) {
        if (typeof value === 'string') return value;
        if (value === undefined) return 'undefined';
        if (value === null) return 'null';
        try { return JSON.stringify(value) ?? String(value); }
        catch (_) { return String(value); }
    }
    const console = Object.freeze({
        log: (...values) => __meow_console('log', values.map(formatConsoleValue).join(' ')),
        info: (...values) => __meow_console('info', values.map(formatConsoleValue).join(' ')),
        warn: (...values) => __meow_console('warn', values.map(formatConsoleValue).join(' ')),
        error: (...values) => __meow_console('error', values.map(formatConsoleValue).join(' ')),
    });

    class DOMException extends Error {
        constructor(message = '', name = 'Error') {
            super(String(message));
            this.name = String(name);
        }
    }

    class Headers {
        constructor(init = undefined) {
            this.__entries = [];
            if (init instanceof Headers) {
                for (const [name, value] of init) this.append(name, value);
            } else if (Array.isArray(init)) {
                for (const pair of init) {
                    if (!Array.isArray(pair) || pair.length !== 2) throw new TypeError('invalid header pair');
                    this.append(pair[0], pair[1]);
                }
            } else if (init && typeof init === 'object') {
                for (const name of Object.keys(init)) this.append(name, init[name]);
            }
        }
        append(name, value) {
            name = normalizeHeaderName(name);
            value = normalizeHeaderValue(value);
            const current = this.__entries.find(entry => entry[0] === name);
            if (current) current[1] += ', ' + value;
            else this.__entries.push([name, value]);
        }
        set(name, value) {
            name = normalizeHeaderName(name);
            this.delete(name);
            this.__entries.push([name, normalizeHeaderValue(value)]);
        }
        get(name) {
            name = normalizeHeaderName(name);
            return this.__entries.find(entry => entry[0] === name)?.[1] ?? null;
        }
        has(name) { return this.get(name) !== null; }
        delete(name) {
            name = normalizeHeaderName(name);
            this.__entries = this.__entries.filter(entry => entry[0] !== name);
        }
        entries() { return this.__entries.map(entry => [...entry])[Symbol.iterator](); }
        keys() { return this.__entries.map(entry => entry[0])[Symbol.iterator](); }
        values() { return this.__entries.map(entry => entry[1])[Symbol.iterator](); }
        forEach(callback, thisArg = undefined) {
            for (const [name, value] of this.__entries) callback.call(thisArg, value, name, this);
        }
        [Symbol.iterator]() { return this.entries(); }
        __json() { return this.__entries.map(entry => [...entry]); }
    }

    function normalizeHeaderName(name) {
        name = String(name).trim().toLowerCase();
        if (!name || !/^[!#$%&'*+.^_`|~0-9a-z-]+$/.test(name)) throw new TypeError('invalid header name');
        return name;
    }
    function normalizeHeaderValue(value) {
        value = String(value).trim();
        if (/[^\t\x20-\x7e\x80-\xff]/.test(value)) throw new TypeError('invalid header value');
        return value;
    }

    class AbortSignal extends EventTarget {
        constructor(id) {
            super();
            Object.defineProperty(this, '__meowTargetKey', { value: '@abort:' + id });
            this.__id = id;
            customTargets.set(this.__meowTargetKey, this);
            this.aborted = false;
            this.reason = undefined;
        }
        throwIfAborted() {
            if (this.aborted) throw this.reason;
        }
    }

    class AbortController {
        constructor() {
            this.signal = new AbortSignal(nextAbortSignalId++);
        }
        abort(reason = new DOMException('The operation was aborted', 'AbortError')) {
            if (this.signal.aborted) return;
            this.signal.aborted = true;
            this.signal.reason = reason;
            __meow_abort_fetches(this.signal.__id);
            dispatch(this.signal, new Event('abort'));
        }
    }

    class Request {
        constructor(input, init = {}) {
            const base = input instanceof Request ? input : null;
            this.url = String(base ? base.url : input);
            this.method = String(init.method ?? base?.method ?? 'GET').toUpperCase();
            this.headers = new Headers(init.headers ?? base?.headers);
            this.body = init.body === undefined ? (base?.body ?? null) : normalizeBody(init.body);
            this.mode = String(init.mode ?? base?.mode ?? 'cors');
            this.credentials = String(init.credentials ?? base?.credentials ?? 'same-origin');
            this.redirect = String(init.redirect ?? base?.redirect ?? 'follow');
            this.signal = init.signal ?? base?.signal ?? null;
            if (this.signal !== null && !(this.signal instanceof AbortSignal)) {
                throw new TypeError('signal must be an AbortSignal');
            }
            if ((this.method === 'GET' || this.method === 'HEAD') && this.body !== null) {
                throw new TypeError('GET/HEAD requests cannot have a body');
            }
        }
        clone() { return new Request(this); }
    }

    class Response {
        constructor(body = null, init = {}) {
            this.__body = body === null ? '' : String(body);
            this.bodyUsed = false;
            this.status = Number(init.status ?? 200);
            this.statusText = String(init.statusText ?? '');
            this.headers = new Headers(init.headers);
            this.url = String(init.url ?? '');
            this.redirected = Boolean(init.redirected);
            this.type = String(init.type ?? 'basic');
            this.ok = this.status >= 200 && this.status <= 299;
        }
        async text() { return this.__consume(); }
        async json() { return JSON.parse(this.__consume()); }
        async arrayBuffer() {
            const text = this.__consume();
            return Uint8Array.from([...text].map(character => character.charCodeAt(0) & 255)).buffer;
        }
        clone() {
            if (this.bodyUsed) throw new TypeError('body already used');
            return new Response(this.__body, {
                status: this.status,
                statusText: this.statusText,
                headers: this.headers,
                url: this.url,
                redirected: this.redirected,
                type: this.type,
            });
        }
        __consume() {
            if (this.bodyUsed) throw new TypeError('body already used');
            this.bodyUsed = true;
            return this.__body;
        }
        static json(data, init = {}) {
            const headers = new Headers(init.headers);
            if (!headers.has('content-type')) headers.set('content-type', 'application/json');
            return new Response(JSON.stringify(data), { ...init, headers });
        }
    }

    function normalizeBody(body) {
        if (body === null || body === undefined) return null;
        if (typeof body === 'string') return body;
        if (typeof URLSearchParams !== 'undefined' && body instanceof URLSearchParams) return body.toString();
        if (ArrayBuffer.isView(body)) return String.fromCharCode(...new Uint8Array(body.buffer, body.byteOffset, body.byteLength));
        if (body instanceof ArrayBuffer) return String.fromCharCode(...new Uint8Array(body));
        return String(body);
    }

    function fetch(input, init = undefined) {
        let request;
        try {
            request = new Request(input, init);
            if (request.signal?.aborted) return Promise.reject(request.signal.reason);
        } catch (error) {
            return Promise.reject(error);
        }
        const descriptor = {
            url: request.url,
            method: request.method,
            headers: request.headers.__json(),
            body: request.body,
            mode: request.mode,
            credentials: request.credentials,
            redirect: request.redirect,
            signalId: request.signal?.__id ?? null,
        };
        const id = __meow_enqueue_fetch(JSON.stringify(descriptor));
        return new Promise((resolve, reject) => fetchPromises.set(id, { resolve, reject }));
    }

    function completeFetch(id, payload) {
        const pending = fetchPromises.get(id);
        if (!pending) return;
        fetchPromises.delete(id);
        if (!payload.ok) {
            const error = payload.name === 'AbortError'
                ? new DOMException(payload.error, 'AbortError')
                : new TypeError(payload.error);
            pending.reject(error);
            return;
        }
        pending.resolve(new Response(payload.body, payload.response));
    }

    class Storage {
        constructor(kind) { this.__kind = kind; }
        get length() { return __meow_storage_length(this.__kind); }
        key(index) { return __meow_storage_key(this.__kind, Number(index)); }
        getItem(key) { return __meow_storage_get(this.__kind, String(key)); }
        setItem(key, value) { __meow_storage_set(this.__kind, String(key), String(value)); }
        removeItem(key) { __meow_storage_remove(this.__kind, String(key)); }
        clear() { __meow_storage_clear(this.__kind); }
    }

    class MessageEvent extends Event {
        constructor(type, init = {}) {
            super(type, init);
            this.data = init.data;
            this.origin = String(init.origin ?? '');
        }
    }
    class CloseEvent extends Event {
        constructor(type, init = {}) {
            super(type, init);
            this.code = Number(init.code ?? 0);
            this.reason = String(init.reason ?? '');
            this.wasClean = Boolean(init.wasClean);
        }
    }

    function installEventHandler(prototype, type) {
        const slot = '__on' + type;
        Object.defineProperty(prototype, 'on' + type, {
            get() { return this[slot] ?? null; },
            set(callback) {
                if (this[slot]) this.removeEventListener(type, this[slot]);
                this[slot] = typeof callback === 'function' ? callback : null;
                if (this[slot]) this.addEventListener(type, this[slot]);
            },
        });
    }

    class WebSocket extends EventTarget {
        constructor(url, protocols = []) {
            super();
            this.url = String(url);
            this.protocol = '';
            this.extensions = '';
            this.binaryType = 'arraybuffer';
            this.bufferedAmount = 0;
            this.readyState = WebSocket.CONNECTING;
            const normalizedProtocols = typeof protocols === 'string' ? [protocols] : [...protocols].map(String);
            this.__id = __meow_websocket_command(JSON.stringify({
                kind: 'connect',
                url: this.url,
                protocols: normalizedProtocols,
            }));
            Object.defineProperty(this, '__meowTargetKey', { value: '@ws:' + this.__id });
            webSockets.set(this.__id, this);
            customTargets.set(this.__meowTargetKey, this);
        }
        send(data) {
            if (this.readyState !== WebSocket.OPEN) throw new DOMException('WebSocket is not open', 'InvalidStateError');
            let descriptor;
            if (typeof data === 'string') {
                descriptor = { kind: 'sendText', id: this.__id, data };
            } else if (data instanceof ArrayBuffer || ArrayBuffer.isView(data)) {
                const bytes = data instanceof ArrayBuffer
                    ? new Uint8Array(data)
                    : new Uint8Array(data.buffer, data.byteOffset, data.byteLength);
                descriptor = { kind: 'sendBinary', id: this.__id, data: Array.from(bytes) };
            } else {
                descriptor = { kind: 'sendText', id: this.__id, data: String(data) };
            }
            __meow_websocket_command(JSON.stringify(descriptor));
        }
        close(code = 1000, reason = '') {
            if (this.readyState === WebSocket.CLOSED || this.readyState === WebSocket.CLOSING) return;
            this.readyState = WebSocket.CLOSING;
            __meow_websocket_command(JSON.stringify({ kind: 'close', id: this.__id, code: Number(code), reason: String(reason) }));
        }
    }
    Object.defineProperties(WebSocket, {
        CONNECTING: { value: 0 }, OPEN: { value: 1 }, CLOSING: { value: 2 }, CLOSED: { value: 3 },
    });
    Object.defineProperties(WebSocket.prototype, {
        CONNECTING: { value: 0 }, OPEN: { value: 1 }, CLOSING: { value: 2 }, CLOSED: { value: 3 },
    });
    for (const type of ['open', 'message', 'error', 'close']) installEventHandler(WebSocket.prototype, type);

    function websocketEvent(id, payload) {
        const socket = webSockets.get(id);
        if (!socket) return;
        switch (payload.kind) {
            case 'open':
                socket.readyState = WebSocket.OPEN;
                socket.protocol = payload.protocol ?? '';
                dispatch(socket, new Event('open'));
                break;
            case 'text':
                dispatch(socket, new MessageEvent('message', { data: payload.data, origin: payload.origin }));
                break;
            case 'binary':
                dispatch(socket, new MessageEvent('message', { data: Uint8Array.from(payload.data).buffer, origin: payload.origin }));
                break;
            case 'error':
                dispatch(socket, new Event('error'));
                break;
            case 'close':
                socket.readyState = WebSocket.CLOSED;
                dispatch(socket, new CloseEvent('close', payload));
                webSockets.delete(id);
                customTargets.delete(socket.__meowTargetKey);
                break;
        }
    }

    const document = new Document();
    const location = new Location();
    for (const name of ['addEventListener', 'removeEventListener', 'dispatchEvent']) {
        Object.defineProperty(globalThis, name, {
            value: EventTarget.prototype[name],
            writable: true,
            configurable: true,
        });
    }
    Object.defineProperties(globalThis, {
        Event: { value: Event, writable: true, configurable: true },
        EventTarget: { value: EventTarget, writable: true, configurable: true },
        Node: { value: Node, writable: true, configurable: true },
        Element: { value: Element, writable: true, configurable: true },
        Document: { value: Document, writable: true, configurable: true },
        document: { value: document, writable: false, configurable: false },
        location: { value: location, writable: false, configurable: false },
        console: { value: console, writable: false, configurable: false },
        setTimeout: { value: setTimeout, writable: true, configurable: true },
        clearTimeout: { value: clearTimer, writable: true, configurable: true },
        setInterval: { value: setInterval, writable: true, configurable: true },
        clearInterval: { value: clearTimer, writable: true, configurable: true },
        queueMicrotask: { value: queueMicrotask, writable: true, configurable: true },
        DOMException: { value: DOMException, writable: true, configurable: true },
        Headers: { value: Headers, writable: true, configurable: true },
        Request: { value: Request, writable: true, configurable: true },
        Response: { value: Response, writable: true, configurable: true },
        AbortSignal: { value: AbortSignal, writable: true, configurable: true },
        AbortController: { value: AbortController, writable: true, configurable: true },
        fetch: { value: fetch, writable: true, configurable: true },
        Storage: { value: Storage, writable: true, configurable: true },
        localStorage: { value: new Storage('local'), writable: false, configurable: false },
        sessionStorage: { value: new Storage('session'), writable: false, configurable: false },
        MessageEvent: { value: MessageEvent, writable: true, configurable: true },
        CloseEvent: { value: CloseEvent, writable: true, configurable: true },
        WebSocket: { value: WebSocket, writable: true, configurable: true },
        window: { value: globalThis, writable: false, configurable: false },
        __meow_complete_fetch: { value: completeFetch },
        __meow_websocket_event: { value: websocketEvent },
        __meow_dispatch_trusted: {
            value: (id, type, bubbles, cancelable) => dispatch(
                wrap(id),
                new Event(type, { bubbles, cancelable }),
            ),
        },
        __meow_fire_timer: {
            value: id => {
                const timer = timerCallbacks.get(id);
                if (!timer) return;
                if (!timer.repeat) timerCallbacks.delete(id);
                timer.callback(...timer.args);
            },
        },
    });
})();
