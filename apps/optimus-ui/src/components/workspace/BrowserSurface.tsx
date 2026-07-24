import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import type { BrowserAnnotation, BrowserState, OptimusTransport } from '../../ipc/contracts';
import { frameCoordinator } from '../../performance/frameCoordinator';
import { Icon } from '../chrome/Icon';

const initialState: BrowserState = {
  url: 'https://www.google.com/',
  title: 'Preview',
  loading: false,
  canGoBack: false,
  canGoForward: false,
  visible: false,
  native: false,
};

export function BrowserSurface({
  transport,
  active,
  onAnnotation,
}: {
  transport: OptimusTransport;
  active: boolean;
  onAnnotation: (text: string) => void;
}) {
  const hole = useRef<HTMLDivElement>(null);
  const [state, setState] = useState(initialState);
  const [address, setAddress] = useState(initialState.url);
  const [annotationMode, setAnnotationMode] = useState(false);
  const lastBounds = useRef('');
  const editingAddress = useRef(false);
  const syncGeometry = useRef<(() => void) | null>(null);

  useEffect(() => {
    let alive = true;
    let unsubscribe: () => void = () => undefined;
    if (transport.browser) {
      transport.browser.state().then((next) => {
        if (!alive) return;
        setState(next);
        setAddress(next.url || initialState.url);
      }).catch(() => undefined);
      unsubscribe = transport.browser.subscribe((next) => {
        if (!alive) return;
        setState(next);
        if (next.url && !editingAddress.current) setAddress(next.url);
      });
    }
    return () => {
      alive = false;
      unsubscribe();
    };
  }, [transport]);

  useEffect(() => () => {
    if (annotationMode && state.native) {
      void transport.browser?.cancelAnnotation();
    }
  }, [annotationMode, state.native, transport]);

  useEffect(() => {
    if (active || !annotationMode) return;
    setAnnotationMode(false);
    if (state.native) void transport.browser?.cancelAnnotation();
  }, [active, annotationMode, state.native, transport]);

  useLayoutEffect(() => {
    const node = hole.current;
    if (!node || !transport.browser || !active) return;
    const sync = (reveal = false) => {
      frameCoordinator.schedule('native-geometry', () => {
        const rect = node.getBoundingClientRect();
        const bounds = {
          x: Math.round(rect.left),
          y: Math.round(rect.top),
          width: Math.round(rect.width),
          height: Math.round(rect.height),
        };
        const signature = `${bounds.x}:${bounds.y}:${bounds.width}:${bounds.height}`;
        if (signature !== lastBounds.current) {
          lastBounds.current = signature;
          transport.browser?.setBounds(bounds);
        }
        if (reveal) transport.browser?.setVisible(true);
      });
    };
    const observer = new ResizeObserver(() => sync());
    observer.observe(node);
    const stage = node.closest('.app-stage');
    const shell = node.closest('.workspace-shell');
    if (stage) observer.observe(stage);
    if (shell) observer.observe(shell);
    const onWindowResize = () => sync();
    window.addEventListener('resize', onWindowResize);
    // Geometry and reveal share the latest-value lane so a later state event
    // cannot replace the initial reveal with a bounds-only job.
    syncGeometry.current = () => sync(true);
    sync(true);
    return () => {
      transport.browser?.setVisible(false);
      observer.disconnect();
      window.removeEventListener('resize', onWindowResize);
      syncGeometry.current = null;
      lastBounds.current = '';
    };
  }, [active, transport]);

  useLayoutEffect(() => {
    if (active) syncGeometry.current?.();
  }, [active, state.loading, state.title, state.url]);

  const navigate = async () => {
    let url = address.trim();
    if (!url) return;
    if (!/^[a-z][a-z0-9+.-]*:/i.test(url)) url = `https://${url}`;
    try {
      if (transport.browser) {
        setState(await transport.browser.navigate(url));
      } else {
        const result = await transport.invoke<Record<string, unknown>>('browser_navigate', { url });
        setState({
          ...state,
          url: String(result.url || result.final_url || url),
          title: String(result.title || 'Preview'),
          loading: false,
          native: false,
        });
      }
    } catch (error) {
      setState({
        ...state,
        loading: false,
        error: error instanceof Error ? error.message : String(error),
      });
    }
  };

  const toggleAnnotation = async () => {
    if (annotationMode) {
      setAnnotationMode(false);
      if (state.native) await transport.browser?.cancelAnnotation();
      return;
    }
    setAnnotationMode(true);
    if (!state.native || !transport.browser) return;
    try {
      const result = await transport.browser.annotate();
      if (!result.cancelled) onAnnotation(formatAnnotation(result));
    } finally {
      setAnnotationMode(false);
    }
  };

  return (
    <section className="browser-surface" aria-label="Preview browser">
      <div className="browser-chrome" role="toolbar" aria-label="Browser navigation">
        <button
          type="button"
          aria-label="Back"
          disabled={!state.canGoBack}
          onClick={() => transport.browser?.back().then(setState)}
        >
          <Icon name="back" />
        </button>
        <button
          type="button"
          aria-label="Forward"
          disabled={!state.canGoForward}
          onClick={() => transport.browser?.forward().then(setState)}
        >
          <Icon name="forward" />
        </button>
        <button
          type="button"
          aria-label="Reload"
          onClick={() => transport.browser?.reload().then(setState)}
        >
          <Icon name="reload" />
        </button>
        <form
          className="browser-address"
          onSubmit={(event) => {
            event.preventDefault();
            void navigate();
          }}
        >
          <span className="address-security" aria-hidden="true">⌁</span>
          <input
            aria-label="Browser address"
            value={address}
            onFocus={() => {
              editingAddress.current = true;
            }}
            onBlur={() => {
              editingAddress.current = false;
            }}
            onChange={(event) => setAddress(event.target.value)}
            spellCheck={false}
          />
        </form>
        <button
          type="button"
          className={annotationMode ? 'is-active' : ''}
          aria-pressed={annotationMode}
          aria-label="Annotate preview"
          title="Annotate preview"
          onClick={() => void toggleAnnotation()}
        >
          <Icon name="annotation" />
        </button>
      </div>
      <div
        ref={hole}
        className={`browser-hole${state.native ? ' is-native' : ' is-fixture'}${annotationMode ? ' is-annotating' : ''}`}
        data-testid="browser-hole"
        onClick={(event) => {
          if (!annotationMode || state.native) return;
          const target = event.target as HTMLElement;
          onAnnotation(`Preview annotation: ${target.innerText?.slice(0, 180) || target.tagName}`);
          setAnnotationMode(false);
        }}
      >
        {!state.native ? <FixturePage loading={state.loading} /> : null}
        {state.error ? (
          <div className="browser-error">
            <Icon name="warning" />
            <strong>Could not open this page</strong>
            <span>{state.error}</span>
            <button type="button" onClick={() => void navigate()}>Retry</button>
          </div>
        ) : null}
      </div>
      {annotationMode ? (
        <div className="annotation-hint">Select one element to add bounded preview context.</div>
      ) : null}
    </section>
  );
}

function FixturePage({ loading }: { loading: boolean }) {
  return (
    <div className={`fixture-page${loading ? ' is-loading' : ''}`} aria-label="Deterministic browser fixture">
      <header>
        <a>About</a>
        <a>Store</a>
        <span />
        <a>Gmail</a>
        <a>Images</a>
        <button type="button">Sign in</button>
      </header>
      <main>
        <div className="fixture-logo">
          <span>G</span><span>o</span><span>o</span><span>g</span><span>l</span><span>e</span>
        </div>
        <div className="fixture-search">
          <Icon name="search" />
          <span />
          <span>⌕</span>
        </div>
        <div className="fixture-actions">
          <button type="button">Google Search</button>
          <button type="button">I’m Feeling Lucky</button>
        </div>
        <small>Google offered in: Māori</small>
      </main>
      <footer>
        <span>New Zealand</span>
        <nav><a>Advertising</a><a>Business</a><a>Privacy</a><a>Terms</a></nav>
      </footer>
    </div>
  );
}

function safeHost(url: string) {
  try {
    return new URL(url).host;
  } catch {
    return url || 'idle';
  }
}

function formatAnnotation(annotation: BrowserAnnotation) {
  const kind = annotation.role || annotation.tag || 'element';
  const label = annotation.label || annotation.text || 'Unlabelled element';
  const host = safeHost(annotation.url || '');
  const size = annotation.rect
    ? `, ${annotation.rect.width} × ${annotation.rect.height}px`
    : '';
  return `Preview context: ${kind} “${label}” on ${host || 'the current page'}${size}.`;
}
