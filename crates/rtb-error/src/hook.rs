//! Hook installation helpers.
//!
//! `miette` stores its error hook in a process-global [`OnceLock`], which
//! means `miette::set_hook` succeeds only on the first call and refuses
//! subsequent attempts with `InstallError`. To give callers mutable,
//! idempotent semantics — swap the footer at any time, call
//! `install_*` twice without panicking — we install a single wrapper
//! handler that reads from our own, mutable footer slot at render time.
//!
//! The net effect for callers: all three `install_*` functions are safe
//! to call in any order, any number of times.

use std::fmt;
use std::sync::{OnceLock, RwLock};

use miette::{Diagnostic, GraphicalReportHandler, ReportHandler};

type Footer = Box<dyn Fn() -> String + Send + Sync + 'static>;

/// Global footer slot. Read on every diagnostic render.
static FOOTER: OnceLock<RwLock<Option<Footer>>> = OnceLock::new();

fn footer_slot() -> &'static RwLock<Option<Footer>> {
    FOOTER.get_or_init(|| RwLock::new(None))
}

/// Install the default `miette` graphical report handler.
///
/// Idempotent. Safe to call from `main()` before `tokio::main`
/// expansion or from inside an `Application::run()` invocation.
///
/// If another caller (including a previous call to this function, to
/// [`install_with_footer`], or to `miette::set_hook` directly) has
/// already installed a hook, this call is a no-op — the existing hook
/// is preserved.
pub fn install_report_handler() {
    // Prime the footer slot so concurrent callers can't observe a
    // half-initialised state when we install the hook below.
    let _ = footer_slot();

    let _ = miette::set_hook(Box::new(|_| Box::new(RtbReportHandler::new())));
}

/// Install the `miette` panic hook, routing panics through the same
/// graphical report pipeline.
///
/// Idempotent — `std::panic::set_hook` simply overwrites any previous
/// hook, so calling twice leaves miette's renderer in place.
pub fn install_panic_hook() {
    miette::set_panic_hook();
}

/// Install the report handler (if not already) and register a closure
/// that appends a tool-specific support footer to every rendered
/// diagnostic.
///
/// `footer` is called on every diagnostic render and may return an
/// empty string to suppress the footer for that render. Replacing the
/// footer is permitted — the most recent call wins.
pub fn install_with_footer<F>(footer: F)
where
    F: Fn() -> String + Send + Sync + 'static,
{
    install_report_handler();
    let mut guard =
        footer_slot().write().expect("footer lock poisoned — another thread panicked mid-update");
    *guard = Some(Box::new(footer));
}

/// `rtb-error`'s `ReportHandler` implementation.
///
/// Delegates the structural render to `miette`'s `GraphicalReportHandler`
/// and appends the currently-registered footer, if any. The footer is
/// resolved at render time, not install time, so post-install updates
/// are visible immediately.
struct RtbReportHandler {
    inner: GraphicalReportHandler,
}

impl RtbReportHandler {
    fn new() -> Self {
        Self { inner: GraphicalReportHandler::new() }
    }
}

impl ReportHandler for RtbReportHandler {
    fn debug(&self, diagnostic: &dyn Diagnostic, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.render_report(f, diagnostic)?;
        if let Some(slot) = FOOTER.get() {
            if let Ok(guard) = slot.read() {
                if let Some(footer) = guard.as_ref() {
                    let text = footer();
                    if !text.is_empty() {
                        writeln!(f)?;
                        writeln!(f, "{text}")?;
                    }
                }
            }
        }
        Ok(())
    }
}
