//! What a session with an agent cost, so two of them can be compared.
//!
//! `vault/07-Adopcion-y-Fases/Agentes-Externos.md` says what this is for and,
//! more importantly, what it is not for. It is **not** telemetry: nothing is
//! sent anywhere, nothing is sampled, and it exists to answer one question that
//! the project cannot otherwise answer at all — *does the same model do better
//! work here than it does with `cat`, `grep` and `sed`?*
//!
//! ## Only what was actually observed
//!
//! Every field here is counted on this side of the wire, where it is a fact.
//! Token usage is deliberately absent: this process never sees it, and the only
//! way to have it would be to estimate one — which would be a number that looks
//! like a measurement and is a guess. `dev/bench-external-agent.sh` captures it
//! from the agent's own JSON where the agent prints one, and leaves it out where
//! it does not. Rule 10 of `CLAUDE.md`: a failure to read is not a failure to
//! exist, and neither is written down as the other.

use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

pub struct Metrics {
    file: Option<PathBuf>,
    began: Instant,
    calls: u64,
    errors: u64,
    /// Refusals from the machine, which are not the same as a broken tool call.
    /// An agent that asked for something outside the workspace was answered; an
    /// agent whose tool crashed was not.
    refusals: u64,
    bytes_returned: u64,
    per_tool: BTreeMap<String, u64>,
    /// The three that answer the question this exists for. A run where the
    /// agent read forty files and never asked the index is a run where the
    /// index bought nothing, and no total can show that.
    files_read: u64,
    text_searches: u64,
    index_questions: u64,
    attempts_begun: u64,
    attempts_committed: u64,
    attempts_abandoned: u64,
}

impl Metrics {
    pub fn new(file: Option<PathBuf>) -> Self {
        Self {
            file,
            began: Instant::now(),
            calls: 0,
            errors: 0,
            refusals: 0,
            bytes_returned: 0,
            per_tool: BTreeMap::new(),
            files_read: 0,
            text_searches: 0,
            index_questions: 0,
            attempts_begun: 0,
            attempts_committed: 0,
            attempts_abandoned: 0,
        }
    }

    /// One finished tool call.
    pub fn call(
        &mut self,
        tool: &str,
        arguments: &Value,
        bytes: usize,
        failed: bool,
        refused: bool,
    ) {
        self.calls += 1;
        self.bytes_returned += bytes as u64;
        *self.per_tool.entry(tool.to_string()).or_default() += 1;
        if failed {
            self.errors += 1;
        }
        if refused {
            self.refusals += 1;
        }
        match tool {
            "thalyx_read" => self.files_read += 1,
            "thalyx_find" => self.text_searches += 1,
            "thalyx_symbol" | "thalyx_dependencies" => self.index_questions += 1,
            "thalyx_attempt" => match arguments.get("action").and_then(Value::as_str) {
                Some("begin") => self.attempts_begun += 1,
                Some("commit") => self.attempts_committed += 1,
                // Counted on the call that carried the confirmation, because
                // the first `abandon` is a question and only the second undoes
                // anything. Counting both would double every abandon.
                Some("abandon")
                    if arguments.get("confirm").and_then(Value::as_bool) == Some(true) =>
                {
                    self.attempts_abandoned += 1
                }
                _ => {}
            },
            _ => {}
        }
        self.write();
    }

    pub fn object(&self) -> Value {
        json!({
            "wall_seconds": self.began.elapsed().as_secs_f64(),
            "mcp_calls": self.calls,
            "tools_used": self.per_tool,
            "bytes_returned": self.bytes_returned,
            "errors": self.errors,
            "refusals": self.refusals,
            "files_read": self.files_read,
            "text_searches": self.text_searches,
            "index_questions": self.index_questions,
            "attempts": {
                "begun": self.attempts_begun,
                "committed": self.attempts_committed,
                "abandoned": self.attempts_abandoned,
            },
            // Said out loud rather than left absent, so that a run whose summary
            // has no token count is a run where nobody could count them and not
            // one where somebody forgot to look.
            "tokens": "not observable from this side of the wire",
        })
    }

    /// Rewrite the summary after every call.
    ///
    /// Not at exit. An MCP server is killed by its client when the client is
    /// done, and often without a signal it can act on — a summary written only
    /// on the way out is a summary that is missing exactly when a run ended
    /// badly, which is the run worth looking at.
    fn write(&self) {
        let Some(file) = &self.file else { return };
        let _ = write_atomically(file, &self.object());
    }
}

fn write_atomically(file: &Path, object: &Value) -> std::io::Result<()> {
    let scratch = file.with_extension("partial");
    std::fs::write(&scratch, serde_json::to_vec_pretty(object)?)?;
    // Renamed rather than written in place: a reader that opened the file
    // between the truncate and the write would find nothing and conclude the
    // session did nothing.
    std::fs::rename(scratch, file)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reading_run_and_an_indexing_run_do_not_look_the_same() {
        // The whole point of the three counters: two runs with the same number
        // of calls, where one used the index and the other read files, must be
        // distinguishable — otherwise the comparison this file exists for cannot
        // be made.
        let mut reading = Metrics::new(None);
        let mut indexing = Metrics::new(None);
        for _ in 0..4 {
            reading.call("thalyx_read", &json!({}), 100, false, false);
            indexing.call("thalyx_symbol", &json!({}), 100, false, false);
        }
        assert_eq!(reading.object()["files_read"], json!(4));
        assert_eq!(reading.object()["index_questions"], json!(0));
        assert_eq!(indexing.object()["index_questions"], json!(4));
        assert_eq!(indexing.object()["files_read"], json!(0));
    }

    #[test]
    fn an_abandon_is_counted_once_and_on_the_call_that_did_it() {
        let mut metrics = Metrics::new(None);
        metrics.call(
            "thalyx_attempt",
            &json!({"action": "abandon"}),
            10,
            false,
            false,
        );
        assert_eq!(metrics.object()["attempts"]["abandoned"], json!(0));
        metrics.call(
            "thalyx_attempt",
            &json!({"action": "abandon", "confirm": true}),
            10,
            false,
            false,
        );
        assert_eq!(metrics.object()["attempts"]["abandoned"], json!(1));
    }

    #[test]
    fn a_refusal_and_a_broken_call_are_counted_apart() {
        // Rule 10. An agent told "that is outside the workspace" was answered;
        // an agent whose call could not be made was not, and a summary that
        // merged them would hide a broken adapter behind a careful agent.
        let mut metrics = Metrics::new(None);
        metrics.call("thalyx_read", &json!({}), 10, true, true);
        metrics.call("thalyx_read", &json!({}), 10, true, false);
        assert_eq!(metrics.object()["errors"], json!(2));
        assert_eq!(metrics.object()["refusals"], json!(1));
    }

    #[test]
    fn the_summary_is_on_disk_before_the_session_ends() {
        // Written after every call, because a client that kills its server is
        // the ordinary way one of these ends.
        let root = tempfile::tempdir().expect("tempdir");
        let file = root.path().join("metrics.json");
        let mut metrics = Metrics::new(Some(file.clone()));
        metrics.call("thalyx_state", &json!({}), 42, false, false);
        let written: Value =
            serde_json::from_slice(&std::fs::read(&file).expect("read")).expect("json");
        assert_eq!(written["mcp_calls"], json!(1));
        assert_eq!(written["bytes_returned"], json!(42));
    }
}
