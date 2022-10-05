use camino::Utf8PathBuf;
use chrono::Utc;
use color_eyre::eyre;
use color_eyre::eyre::eyre;
use color_eyre::eyre::Context;
use tracing::metadata::LevelFilter;
use tracing::Level;
use tracing_subscriber::filter::FilterFn;
use tracing_subscriber::fmt;
use tracing_subscriber::fmt::format::JsonFields;
use tracing_subscriber::prelude::*;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::Layer;

mod format;

/// Initialize the logging framework.
///
/// Returns the path logs are being written to.
pub fn install_tracing(filter_directives: &str) -> eyre::Result<Utf8PathBuf> {
    let env_filter = EnvFilter::try_new(filter_directives)
        .or_else(|_| EnvFilter::try_from_default_env())
        .or_else(|_| EnvFilter::try_new("info"))?;

    let fmt_layer = fmt::layer()
        .event_format(format::EventFormatter::default())
        .with_filter(env_filter);

    let (json_layer, log_path) = tracing_json_layer()?;

    let registry = tracing_subscriber::registry();

    registry.with(json_layer).with(fmt_layer).init();

    Ok(log_path)
}

fn tracing_log_file_path() -> eyre::Result<Utf8PathBuf> {
    let mut path = Utf8PathBuf::from_path_buf(
        dirs::cache_dir().ok_or_else(|| eyre!("Could not locate cache directory"))?,
    )
    .map_err(|path| eyre!("Cache directory path contains invalid UTF-8: {path:?}"))?;
    path.push("ava-apartment-finder");

    std::fs::create_dir_all(&path)?;

    let format = "ava-apartment-finder-%FT%H_%M_%S%z.jsonl";
    path.push(&Utc::now().format(&format).to_string());
    Ok(path)
}

fn tracing_json_layer<S>() -> eyre::Result<(
    Box<dyn tracing_subscriber::Layer<S> + Send + Sync + 'static>,
    Utf8PathBuf,
)>
where
    S: tracing::Subscriber + for<'a> tracing_subscriber::registry::LookupSpan<'a>,
{
    let path = tracing_log_file_path().wrap_err("Failed to create log path")?;
    let file = std::fs::File::create(&path).wrap_err_with(|| format!("Failed to open {path:?}"))?;

    let layer = fmt::layer()
        .event_format(fmt::format::json())
        .fmt_fields(JsonFields::new())
        .with_writer(file)
        .with_filter(
            FilterFn::new(|metadata| {
                metadata.level() <= &Level::DEBUG && {
                    let target = metadata.target();
                    target.starts_with("ava_apartment_finder") || target.starts_with("jmap")
                }
            })
            .with_max_level_hint(LevelFilter::DEBUG),
        )
        .boxed();

    Ok((layer, path))
}
