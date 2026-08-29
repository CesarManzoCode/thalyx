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
    /// What the agent spent saying what it wanted. Counted separately from what
    /// came back because they answer different halves of the same question: a
    /// surface that costs little to ask and answers a lot is the whole claim.
    bytes_sent: u64,
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
    /// How many questions actually went to the machine, which is **not**
    /// `calls`: one tool call can be several — `thalyx_state` is three, and a
    /// change that opens its own attempt is two.
    machine_requests: u64,
    /// How long those questions spent between leaving this process and being
    /// answered.
    ///
    /// The one number that separates the bridge from everything else, and it is
    /// named for what it can honestly see: framing, the socket, the guest's
    /// round trip and the verb's own work, all together. Splitting those four
    /// needs an instrument inside the machine, and a field that claimed to have
    /// split them from out here would be a guess wearing a measurement's name.
    /// What is left — `wall_seconds` minus this — is this adapter's own cost
    /// plus every second the model spent thinking.
    machine_seconds: f64,
    /// Calls that changed the workspace and came back without an error.
    ///
    /// This is the only instrument on the whole comparison that can say arm B
    /// really wrote something. Its workspace lives inside the machine, so the
    /// host cannot walk it during a run the way it walks arm A's copy — and
    /// `dev/bench-summary.py` used to settle the question by looking for the
    /// new name in a tool call, which a `thalyx_find` for that name satisfies
    /// without touching a byte.
    ///
    /// Counted on this side of the wire and only for calls the machine
    /// answered: a refused call and a failed call changed nothing, and an
    /// `edit show` returns numbered lines. Rule 9 — the flattering direction
    /// is the one this must never be wrong in.
    mutations: u64,
    /// ── the ratio this whole direction is a bet on ──────────────────────
    ///
    /// `vault/09-Notas-Tecnicas/Trabajo-Entre-Inferencias.md`. Every number
    /// above counts what the *agent* did. These count what the **machine** did
    /// between two of those, which is the quantity the hypothesis is about: a
    /// transaction that performs thirty operations for one round trip is the
    /// win, and no total of calls can show it — a run that got more done per
    /// call looks, in `mcp_calls` alone, exactly like a run that did less.
    ///
    /// Read out of the machine's own answer rather than inferred here. That is
    /// measurement and not interpretation: nothing is decided by it, it changes
    /// no request, and the alternative — this process counting the steps it
    /// sent — would count what was *asked for* rather than what happened, and
    /// would keep counting after a program stopped at its second step.
    programs_run: u64,
    machine_operations: u64,
    /// Bytes the machine produced inside a program and did not send back.
    internal_bytes: u64,
    programs_committed: u64,
    programs_rolled_back: u64,
}

/// Whether an `abandon` call is the one that actually undoes something.
///
/// Two shapes say yes, and they are not interchangeable: the second call of the
/// two-step protocol carries `confirm`, and the one-call form carries the
/// attempt it is settling together with what abandoning it costs. A counter that
/// knew only the first would report zero abandons for an agent using the second
/// — which is the failure this function exists to make impossible to bring back
/// quietly.
fn consented(arguments: &Value) -> bool {
    if arguments.get("confirm").and_then(Value::as_bool) == Some(true) {
        return true;
    }
    ["snapshot", "state"]
        .iter()
        .all(|named| arguments.get(named).is_some())
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
            bytes_sent: 0,
            per_tool: BTreeMap::new(),
            files_read: 0,
            text_searches: 0,
            index_questions: 0,
            attempts_begun: 0,
            attempts_committed: 0,
            attempts_abandoned: 0,
            machine_requests: 0,
            machine_seconds: 0.0,
            mutations: 0,
            programs_run: 0,
            machine_operations: 0,
            internal_bytes: 0,
            programs_committed: 0,
            programs_rolled_back: 0,
        }
    }

    /// What one `thalyx_exec` answer said the machine did.
    ///
    /// Every field is read defensively and a missing one adds nothing: an
    /// answer from an older machine must leave these at what they were rather
    /// than at zero, because a counter that reset on a version skew would
    /// report the very thing this measures as having stopped happening.
    pub fn program(&mut self, answer: &Value) {
        let number = |name: &str| answer.get(name).and_then(Value::as_u64).unwrap_or(0);
        self.programs_run += 1;
        self.machine_operations += number("machine_operations");
        self.internal_bytes += number("internal_bytes");
        match answer.get("status").and_then(Value::as_str) {
            Some("committed") => self.programs_committed += 1,
            Some("rolled_back") => self.programs_rolled_back += 1,
            _ => {}
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
        // The arguments as they went over the wire, which is what the agent
        // actually spent. Serialised here rather than measured at the socket
        // because the socket also carries framing this crate did not choose,
        // and a number that mixed the two would not be the agent's cost.
        self.bytes_sent += arguments.to_string().len() as u64;
        *self.per_tool.entry(tool.to_string()).or_default() += 1;
        if failed {
            self.errors += 1;
        }
        if refused {
            self.refusals += 1;
        }
        if !failed && !refused && changes_the_workspace(tool, arguments) {
            self.mutations += 1;
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
                // Counted on the call that carries the consent, whichever of
                // the two shapes it is: the plain `abandon` before it is a
                // question and undoes nothing, so counting that one too would
                // double every abandon. When the one-call form arrived this
                // read `confirm` alone — and would have quietly stopped
                // counting abandons the moment agents started using the shape
                // that needs no `confirm`. Rule 5: the instrument is part of
                // what a change can break.
                Some("abandon") if consented(arguments) => self.attempts_abandoned += 1,
                _ => {}
            },
            _ => {}
        }
        self.write();
    }

    /// One question asked of the machine, and how long it took to answer.
    ///
    /// Recorded per request rather than per tool call, because a tool call is
    /// not a unit of transport: `thalyx_state` is three questions and a change
    /// that opens its own attempt is two, so a mean taken over calls would
    /// describe nothing that exists.
    pub fn asked(&mut self, took: std::time::Duration) {
        self.machine_requests += 1;
        self.machine_seconds += took.as_secs_f64();
    }

    pub fn object(&self) -> Value {
        json!({
            "wall_seconds": self.began.elapsed().as_secs_f64(),
            "machine_requests": self.machine_requests,
            "machine_seconds": self.machine_seconds,
            "mcp_calls": self.calls,
            "tools_used": self.per_tool,
            "bytes_returned": self.bytes_returned,
            "bytes_sent": self.bytes_sent,
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
            "mutations": self.mutations,
            "programs": {
                "run": self.programs_run,
                "committed": self.programs_committed,
                "rolled_back": self.programs_rolled_back,
                "machine_operations": self.machine_operations,
                "internal_bytes": self.internal_bytes,
                // The ratio, worked out here so that nobody reading a summary
                // has to. `null` when no program ran, never zero: "this run
                // used no programs" and "this run's programs did nothing" are
                // different facts and must not share a number.
                "operations_per_request": if self.programs_run == 0 {
                    Value::Null
                } else {
                    json!(self.machine_operations as f64 / self.programs_run as f64)
                },
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

/// Whether one answered call changed the workspace.
///
/// Named rather than inferred, and narrow on purpose: a tool that is not in
/// this list counts as no change, so a tool added later is uncounted until
/// somebody adds it here. That is the safe direction — the number exists to be
/// evidence that arm B wrote something, and a count that guessed high would be
/// evidence for the arm this project has every reason to want to win.
fn changes_the_workspace(tool: &str, arguments: &Value) -> bool {
    match tool {
        // `show` returns numbered lines and writes nothing.
        "thalyx_edit" => arguments.get("action").and_then(Value::as_str) != Some("show"),
        "thalyx_file" => matches!(
            arguments.get("action").and_then(Value::as_str),
            Some("create" | "create_directory" | "delete" | "move" | "copy")
        ),
        // An abandon puts the workspace back, which is a change to it — and it
        // is the change the `reversible` task is about. The unconfirmed first
        // call is a question and is counted nowhere.
        //
        // `consented` and not `confirm` alone: the one-call form carries no
        // `confirm`, and reading only that word would have quietly stopped
        // counting the abandons of every agent that moved to the newer shape.
        "thalyx_attempt" => {
            arguments.get("action").and_then(Value::as_str) == Some("abandon")
                && consented(arguments)
        }
        // A program that ran at all touched the workspace or put it back —
        // both are changes to it, and both are the thing being measured. What
        // it did in detail is `programs`, above.
        "thalyx_exec" => true,
        _ => false,
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
    fn only_a_call_that_changed_something_and_worked_counts_as_a_mutation() {
        // The whole reason this counter exists. `dev/bench-summary.py` used to
        // decide that arm B "really changed" the workspace because the new name
        // appeared in some tool call — which a search for that name satisfies,
        // and so does an edit that failed. Arm B's workspace is inside the
        // machine and cannot be walked from the host during a run, so if this
        // number is wrong there is nothing else to catch it.
        let mut metrics = Metrics::new(None);
        metrics.call(
            "thalyx_find",
            &json!({"query": "WidgetRenamed"}),
            10,
            false,
            false,
        );
        metrics.call(
            "thalyx_edit",
            &json!({"path": "a.rs", "action": "show"}),
            10,
            false,
            false,
        );
        metrics.call(
            "thalyx_edit",
            &json!({"path": "a.rs", "action": "replace", "at": "3", "text": "WidgetRenamed"}),
            10,
            true,
            false,
        );
        metrics.call(
            "thalyx_file",
            &json!({"action": "delete", "path": "../outside"}),
            10,
            true,
            true,
        );
        assert_eq!(metrics.object()["mutations"], json!(0));

        metrics.call(
            "thalyx_edit",
            &json!({"path": "a.rs", "action": "replace", "at": "3", "text": "WidgetRenamed"}),
            10,
            false,
            false,
        );
        metrics.call(
            "thalyx_file",
            &json!({"action": "move", "path": "a.rs", "to": "b.rs"}),
            10,
            false,
            false,
        );
        metrics.call(
            "thalyx_attempt",
            &json!({"action": "abandon", "confirm": true}),
            10,
            false,
            false,
        );
        assert_eq!(metrics.object()["mutations"], json!(3));
    }

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
    fn one_call_that_did_thirty_things_does_not_look_like_one_that_did_one() {
        // **The measurement this session exists to make possible.** Both runs
        // below are one MCP call. Counting calls, they are identical; counting
        // what the machine did between two inferences, one of them did thirty
        // times as much — and that difference is the entire hypothesis.
        let mut busy = Metrics::new(None);
        busy.call("thalyx_exec", &json!({"steps": []}), 400, false, false);
        busy.program(&json!({
            "status": "committed", "machine_operations": 30, "internal_bytes": 90_000
        }));

        let mut plain = Metrics::new(None);
        plain.call("thalyx_edit", &json!({"path": "a.rs"}), 400, false, false);

        assert_eq!(busy.object()["mcp_calls"], plain.object()["mcp_calls"]);
        assert_eq!(
            busy.object()["programs"]["operations_per_request"],
            json!(30.0)
        );
        // `null` and never zero: no program ran, which is a different fact from
        // a program that did nothing.
        assert_eq!(
            plain.object()["programs"]["operations_per_request"],
            Value::Null
        );
    }

    #[test]
    fn an_answer_from_a_machine_that_does_not_carry_the_numbers_leaves_them_alone() {
        // A version skew must not read as the thing being measured having
        // stopped happening.
        let mut metrics = Metrics::new(None);
        metrics.program(&json!({"status": "committed", "machine_operations": 12}));
        metrics.program(&json!({"status": "committed"}));
        assert_eq!(
            metrics.object()["programs"]["machine_operations"],
            json!(12)
        );
        assert_eq!(metrics.object()["programs"]["run"], json!(2));
    }

    #[test]
    fn a_rollback_and_a_commit_are_counted_apart() {
        let mut metrics = Metrics::new(None);
        metrics.program(&json!({"status": "committed"}));
        metrics.program(&json!({"status": "rolled_back"}));
        metrics.program(&json!({"status": "kept_after_failure"}));
        assert_eq!(metrics.object()["programs"]["committed"], json!(1));
        assert_eq!(metrics.object()["programs"]["rolled_back"], json!(1));
    }

    #[test]
    fn the_one_call_abandon_is_counted_as_an_abandon_and_as_a_mutation() {
        // The instrument is part of what a change can break — rule 5. When the
        // one-call form changed from two counts to a state witness, a counter
        // still looking for `delete` and `revert` would have reported zero
        // abandons for every agent using it, and the run would have read as an
        // agent that never undid anything.
        let mut metrics = Metrics::new(None);
        metrics.call(
            "thalyx_attempt",
            &json!({"action": "abandon", "snapshot": "s", "state": "w2-abc"}),
            10,
            false,
            false,
        );
        assert_eq!(metrics.object()["attempts"]["abandoned"], json!(1));
        assert_eq!(metrics.object()["mutations"], json!(1));
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
