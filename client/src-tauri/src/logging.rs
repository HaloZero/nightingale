use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

#[cfg(not(debug_assertions))]
use std::io::Write;
#[cfg(not(debug_assertions))]
use std::sync::{Arc, Mutex};

#[cfg(not(debug_assertions))]
struct LogFileWriter(Arc<Mutex<std::fs::File>>);

#[cfg(not(debug_assertions))]
impl Write for LogFileWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let mut file = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        file.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        let mut file = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        file.flush()
    }
}

pub(crate) fn init() {
    #[cfg(debug_assertions)]
    {
        let filter = EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new("info,app_core=debug,client_lib=debug"));
        let _ = tracing_subscriber::registry()
            .with(filter)
            .with(fmt::layer().with_target(true).with_writer(std::io::stdout))
            .try_init();
    }

    #[cfg(not(debug_assertions))]
    {
        let log_dir = app_core::default_nightingale_dir();
        let _ = std::fs::create_dir_all(&log_dir);
        let log_path = log_dir.join("nightingale.log");

        let file = match std::fs::File::create(&log_path) {
            Ok(f) => f,
            Err(_) => return,
        };

        let shared = Arc::new(Mutex::new(file));
        let writer = shared.clone();

        let filter = EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new("info,app_core=debug,client_lib=debug"));

        let _ = tracing_subscriber::registry()
            .with(filter)
            .with(
                fmt::layer()
                    .with_target(true)
                    .with_ansi(false)
                    .with_writer(move || LogFileWriter(writer.clone())),
            )
            .try_init();

        let _ = writeln!(
            shared
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
            "--- Nightingale log started ---"
        );
    }
}
