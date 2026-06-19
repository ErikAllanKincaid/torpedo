use std::path::PathBuf;

use anyhow::{Context, Result};
use iroh::SecretKey;

fn config_dir() -> Result<PathBuf> {
    let dir = dirs::config_dir()
        .context("could not determine config directory")?
        .join("pitopi");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn key_path() -> Result<PathBuf> {
    Ok(config_dir()?.join("secret_key"))
}

pub fn load_or_create() -> Result<SecretKey> {
    let path = key_path()?;
    if path.exists() {
        let bytes: [u8; 32] = std::fs::read(&path)?
            .try_into()
            .map_err(|_| anyhow::anyhow!("corrupt secret key file"))?;
        let key = SecretKey::from_bytes(&bytes);
        tracing::info!(id = %key.public().fmt_short(), "loaded identity");
        Ok(key)
    } else {
        let key = SecretKey::generate();
        std::fs::write(&path, key.to_bytes())?;
        tracing::info!(id = %key.public().fmt_short(), "generated new identity");
        Ok(key)
    }
}
