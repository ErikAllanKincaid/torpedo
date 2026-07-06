//! CLI handler for `torpedo open <uri>`: dispatches a `torpedo://` deep link to
//! the same join/pair paths the plain `torpedo join`/`torpedo pair` subcommands
//! use.

use crate::*;
use rayfish::deeplink::{TorpedoLink, parse_torpedo_uri};

pub(crate) async fn cmd_open(uri: &str) -> Result<()> {
    match parse_torpedo_uri(uri)? {
        TorpedoLink::Join(code) => ipc_join(&code, None, None, false, false, false).await,
        TorpedoLink::Pair(ticket) => ipc_pair_accept(&ticket).await,
    }
}
