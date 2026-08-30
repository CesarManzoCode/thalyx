// The program stage 59 runs, and the one `exec::tests` runs against the
// directory-backed fake.
//
// **One file, read by both**, because a program copied into a shell script and
// into a Rust test is two programs, and the second one is the one that has the
// typo nobody finds until Fedora. `dev/verify.sh` reads it and so does
// `the_program_verify_runs_is_the_one_that_is_tested_here`.
//
// Read it as the claim of the whole sprint: **it names no file.** The list
// comes from the machine, the choice comes from what each file says, and the
// last decision comes from what a check answered. A `Vec<Step>` that produced
// the same result would have to already contain the three names — which is the
// answer, so producing it is the work this is doing.

// The semantic provider, asked first — so a run says something about what
// confined it whichever way the tree goes. Not asserted on: one of the trees
// this runs against has no `old_api` in it at all, and an assertion here would
// be about which tree rather than about the program.
const what = thalyx.context("old_api");

const listing = thalyx.list("src");
thalyx.assert(listing.ok, "src could not be listed", listing);

const sources = (listing.entries || [])
    .map((entry) => entry.name)
    .filter((name) => name.endsWith(".rs") && name !== "lib.rs")
    .sort();
thalyx.assert(sources.length >= 4, "this tree should have several modules", sources);

// The loop that could not have been written in advance: what is *in* each file
// decides whether it is touched.
const touched = [];
for (const name of sources) {
    const path = "src/" + name;
    const source = thalyx.read(path);
    if (source.ok && source.text.includes("old_api")) {
        thalyx.mustWork(
            thalyx.substitute(path, "old_api", "new_api"),
            "the substitution in " + path + " did not happen"
        );
        touched.push(name);
    }
}

// What the tree says, not what the edits claimed.
const seen = thalyx.changed();
thalyx.assert(
    seen.count === touched.length,
    "the tree shows " + seen.count + " change(s) and the program made " + touched.length,
    seen
);

if (touched.length === 0) {
    return { changed: [], compiled: false, resolution: what.resolution };
}

// And the branch on a validation — the last decision, and the one a static list
// has to hand back to a model to make.
const built = thalyx.validate({ check: "rust", mode: "check" });
if (built.verdict !== "passed") {
    return { changed: touched, compiled: false, gave_up: built.summary };
}

const left = thalyx.grep("old_api");
thalyx.assert(left.total === 0, "old_api is still somewhere", left);

return {
    changed: touched,
    compiled: true,
    still_there: left.total,
    resolution: what.resolution,
};
