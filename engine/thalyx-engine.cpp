// The inference engine as a process that stays alive.
//
// Cesar's decree of 2026-08-28: the engine is the first real module. This is
// what it runs, and it exists because the shape that decree first landed in
// spent the whole cost of a local model on every sentence. `llama-completion`
// is one-shot by construction — it loads a GGUF, answers once, and dies — so
// asking Thalyx a second question re-read 2 GB from disk, rebuilt the context,
// and re-warmed nothing. On the machine that is several seconds per sentence,
// and every one of them is spent on work the previous sentence already did.
//
// So: same llama.cpp, same pinned tag, same static link, same confinement, same
// module. What changes is the shape of the program. It loads the weights once
// and then waits, answering framed requests on a pipe until Thalyx closes it.
//
// ## What this file deliberately does NOT do
//
// - **It does not reimplement inference.** Everything below the protocol is
//   llama.cpp's own `common` library: `common_tokenize`, `common_sampler_*`,
//   `llama_decode`. If a sampler changes behaviour between tags, it changes
//   here too, which is the point of not writing a second one.
// - **It does not open a socket, a port or an HTTP server.** llama.cpp ships
//   one; using it would have meant granting `net/outbound` to the least
//   trusted program on the machine so that two processes on the same host
//   could talk. The channel is a pipe Thalyx already owns.
// - **It does not keep conversation.** See `serve_one`: the KV cache is cleared
//   and the sampler is rebuilt for every request. Residency is about the
//   *weights*, which cost seconds, and not about the context, which costs
//   milliseconds. A model that quietly remembered the previous sentence would
//   be a second conversational state that Thalyx does not know exists, and
//   Thalyx's transcript is the only one that is allowed to matter.
//
// ## The one thing that must never be got wrong: stdout
//
// Thalyx reads response frames from this program's `stdout`, and llama.cpp
// prints to `stdout` — the model card, the load progress, the timings. One
// stray line in the middle of a frame and Thalyx is parsing a model banner as a
// length. Rather than hunt every printf, `main` moves the real stdout to a
// private descriptor and points descriptor 1 at stderr. After that line,
// *everything* any library prints lands on stderr, where Thalyx drains it as
// the module's ordinary output, and descriptor `g_proto` is the protocol and
// nothing else.
//
// ## The protocol
//
// Little-endian, length-prefixed, no text framing anywhere: a completion is
// arbitrary bytes chosen by an untrusted model, and a delimiter it can type is
// a delimiter it can forge.
//
//   ready     (engine → Thalyx, once)  "THR1" u64 load_ms u32 pid u32 threads u32 n_ctx
//   request   (Thalyx → engine)        "THQ1" u32 predict u64 seed
//                                             u32 len + prompt path
//                                             u32 len + grammar path (0 = none)
//   response  (engine → Thalyx)        "THA1" u8 status u64 elapsed_ms u32 len + body
//
// `status` 0 is an answer, 1 is a failure whose body is the reason. The answer
// body is **the prompt this process read, followed by the completion** — which
// is exactly what `llama-completion` wrote to stdout, so nothing above the seam
// in Thalyx changes. That echo is not decoration: `Prompt::answer_in` finds the
// marker the prompt ends with and takes what follows it, and a marker that is
// missing is how Thalyx tells "the model answered badly" apart from "the tool
// never read the prompt". Echoing the bytes actually read keeps that true.

#include "common.h"
#include "sampling.h"
#include "llama.h"

#include <algorithm>
#include <cerrno>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <chrono>
#include <exception>
#include <string>
#include <thread>
#include <vector>

#include <unistd.h>

// The most a single request may name or answer with. A path longer than this is
// not a path, and a body longer than this is a runaway — Thalyx has its own cap
// at 64 KiB, and this one exists so a broken length on the wire cannot make the
// engine allocate a machine's worth of memory before Thalyx gets to refuse it.
static const uint32_t MAX_FIELD = 8u * 1024u * 1024u;

// The real stdout, moved out of the way of every library that prints. See the
// file header.
static int g_proto = -1;

static uint64_t now_ms() {
    using namespace std::chrono;
    return (uint64_t) duration_cast<milliseconds>(steady_clock::now().time_since_epoch()).count();
}

// ─────────────────────────────────────────────────────────────────── the wire

static bool read_exactly(void * into, size_t want) {
    uint8_t * at = (uint8_t *) into;
    while (want > 0) {
        ssize_t got = read(STDIN_FILENO, at, want);
        if (got == 0) {
            return false;                       // Thalyx closed the pipe: time to go.
        }
        if (got < 0) {
            if (errno == EINTR) {
                continue;
            }
            return false;
        }
        at   += got;
        want -= (size_t) got;
    }
    return true;
}

static bool write_exactly(const void * from, size_t len) {
    const uint8_t * at = (const uint8_t *) from;
    while (len > 0) {
        ssize_t put = write(g_proto, at, len);
        if (put < 0) {
            if (errno == EINTR) {
                continue;
            }
            return false;
        }
        at  += put;
        len -= (size_t) put;
    }
    return true;
}

static void put_u8 (std::vector<uint8_t> & out, uint8_t v)  { out.push_back(v); }
static void put_u32(std::vector<uint8_t> & out, uint32_t v) {
    for (int i = 0; i < 4; i++) { out.push_back((uint8_t) (v >> (8 * i))); }
}
static void put_u64(std::vector<uint8_t> & out, uint64_t v) {
    for (int i = 0; i < 8; i++) { out.push_back((uint8_t) (v >> (8 * i))); }
}

static bool get_u32(uint32_t & v) {
    uint8_t b[4];
    if (!read_exactly(b, 4)) { return false; }
    v = (uint32_t) b[0] | ((uint32_t) b[1] << 8) | ((uint32_t) b[2] << 16) | ((uint32_t) b[3] << 24);
    return true;
}

static bool get_u64(uint64_t & v) {
    uint8_t b[8];
    if (!read_exactly(b, 8)) { return false; }
    v = 0;
    for (int i = 0; i < 8; i++) { v |= (uint64_t) b[i] << (8 * i); }
    return true;
}

static bool get_bytes(std::string & into) {
    uint32_t len = 0;
    if (!get_u32(len)) { return false; }
    if (len > MAX_FIELD) { return false; }
    into.resize(len);
    if (len > 0 && !read_exactly(&into[0], len)) { return false; }
    return true;
}

static bool send_frame(const char magic[4], uint8_t status, uint64_t elapsed_ms, const std::string & body) {
    std::vector<uint8_t> out;
    out.insert(out.end(), magic, magic + 4);
    put_u8 (out, status);
    put_u64(out, elapsed_ms);
    put_u32(out, (uint32_t) body.size());
    if (!write_exactly(out.data(), out.size())) { return false; }
    return write_exactly(body.data(), body.size());
}

// ────────────────────────────────────────────────────────────────── the model

static bool slurp(const std::string & path, std::string & into, std::string & why) {
    FILE * f = fopen(path.c_str(), "rb");
    if (!f) {
        why = "could not open " + path + ": " + strerror(errno);
        return false;
    }
    char buf[65536];
    size_t got;
    into.clear();
    while ((got = fread(buf, 1, sizeof(buf), f)) > 0) {
        into.append(buf, got);
        if (into.size() > MAX_FIELD) {
            fclose(f);
            why = path + " is larger than this engine will read";
            return false;
        }
    }
    fclose(f);
    return true;
}

struct Resident {
    llama_model   * model = nullptr;
    llama_context * ctx   = nullptr;
    int32_t         n_batch = 512;
};

// One inference, on a context that remembers nothing from the last one.
//
// The two resets are the whole of "residency does not mean conversation": the
// KV cache is emptied, and the sampler — which is where a grammar's position
// lives — is built fresh and freed at the end. Both are milliseconds against
// the seconds the weights cost, which is why this is the trade that was worth
// making and caching the context would not have been.
static bool serve_one(Resident & r,
                      const std::string & prompt,
                      const std::string & grammar,
                      uint32_t predict,
                      uint64_t seed,
                      std::string & answer,
                      std::string & why) {
    const llama_vocab * vocab = llama_model_get_vocab(r.model);

    llama_memory_clear(llama_get_memory(r.ctx), true);

    std::vector<llama_token> tokens = common_tokenize(vocab, prompt, true, true);
    if (tokens.empty()) {
        why = "the prompt tokenised to nothing";
        return false;
    }

    const uint32_t n_ctx = llama_n_ctx(r.ctx);
    if (tokens.size() + predict > n_ctx) {
        // Said rather than silently truncated. A prompt quietly cut in half is
        // a model answering a question nobody asked, and the marker Thalyx
        // looks for lives at the *end* of the prompt — so the failure would
        // arrive as "the tool never read the prompt" and send whoever reads it
        // to audit llama.cpp.
        why = "the prompt is " + std::to_string(tokens.size()) + " tokens and the context is "
            + std::to_string(n_ctx) + "; raise the engine's --ctx or shorten the prompt";
        return false;
    }

    common_params_sampling sp;
    sp.seed = (uint32_t) seed;
    sp.temp = 0.0f;
    if (!grammar.empty()) {
        sp.grammar = common_grammar(COMMON_GRAMMAR_TYPE_USER, grammar);
    }

    common_sampler * smpl = common_sampler_init(r.model, sp);
    if (!smpl) {
        why = "the sampler would not initialise — the grammar is probably not valid GBNF";
        return false;
    }

    // The prompt, in batches, so a long one does not exceed what one decode
    // takes. Everything but the last batch asks for no logits.
    for (size_t at = 0; at < tokens.size(); at += (size_t) r.n_batch) {
        const size_t take = std::min((size_t) r.n_batch, tokens.size() - at);
        if (llama_decode(r.ctx, llama_batch_get_one(tokens.data() + at, (int32_t) take)) != 0) {
            common_sampler_free(smpl);
            why = "llama_decode failed on the prompt";
            return false;
        }
    }

    // The prompt's own tokens go through the sampler without being counted as
    // generated, which is what advances a user grammar past a prefix. Same call
    // `llama-completion` makes, same `false`.
    for (llama_token t : tokens) {
        common_sampler_accept(smpl, t, false);
    }

    answer = prompt;                          // the echo — see the file header
    for (uint32_t made = 0; made < predict; made++) {
        llama_token id = common_sampler_sample(smpl, r.ctx, -1);
        common_sampler_accept(smpl, id, true);
        if (llama_vocab_is_eog(vocab, id)) {
            break;
        }
        answer += common_token_to_piece(r.ctx, id, false);
        if (answer.size() > MAX_FIELD) {
            break;
        }
        if (llama_decode(r.ctx, llama_batch_get_one(&id, 1)) != 0) {
            common_sampler_free(smpl);
            why = "llama_decode failed while generating";
            return false;
        }
    }

    common_sampler_free(smpl);
    return true;
}

static void usage(const char * me) {
    fprintf(stderr,
        "usage: %s -m <model.gguf> [--ctx N] [--threads N]\n"
        "\n"
        "Loads the weights once and then answers framed requests on stdin until\n"
        "the pipe closes. Thalyx starts this; nobody types it.\n", me);
}

int main(int argc, char ** argv) {
    // Before anything can print. See the file header: descriptor 1 becomes a
    // copy of stderr, and the protocol moves to a descriptor no library knows
    // the number of.
    g_proto = dup(STDOUT_FILENO);
    if (g_proto < 0) {
        fprintf(stderr, "thalyx-engine: could not take stdout: %s\n", strerror(errno));
        return 1;
    }
    if (dup2(STDERR_FILENO, STDOUT_FILENO) < 0) {
        fprintf(stderr, "thalyx-engine: could not move stdout: %s\n", strerror(errno));
        return 1;
    }

    std::string model_path;
    int32_t n_ctx    = 4096;
    int32_t n_threads = 0;

    for (int i = 1; i < argc; i++) {
        const std::string arg = argv[i];
        const bool has_next = i + 1 < argc;
        if ((arg == "-m" || arg == "--model") && has_next) {
            model_path = argv[++i];
        } else if (arg == "--ctx" && has_next) {
            n_ctx = atoi(argv[++i]);
        } else if (arg == "--threads" && has_next) {
            n_threads = atoi(argv[++i]);
        } else {
            usage(argv[0]);
            return 2;
        }
    }
    if (model_path.empty()) {
        usage(argv[0]);
        return 2;
    }

    if (n_threads <= 0) {
        // What this machine actually has, rather than a number somebody typed
        // on a different one. A module runs under a cgroup that may give it
        // less; oversubscribing there costs contention, so Thalyx passes
        // `--threads` and this is only the floor for running it by hand.
        n_threads = (int32_t) std::thread::hardware_concurrency();
        if (n_threads <= 0) {
            n_threads = 4;
        }
    }

    const uint64_t started = now_ms();

    llama_backend_init();

    // Defaults, deliberately: `load_mode` decides whether the weights are
    // mmapped, and the default is the one `llama-completion` has always used —
    // so the engine loads its GGUF exactly the way the tool it replaces did.
    // Which matters for the ceiling: `module_standard` charges page cache to
    // the module's cgroup, and the engine's manifest asks for several times the
    // size of the file because mmapped weights are counted there.
    const llama_model_params mp = llama_model_default_params();

    Resident r;
    r.model = llama_model_load_from_file(model_path.c_str(), mp);
    if (!r.model) {
        fprintf(stderr, "thalyx-engine: could not load %s\n", model_path.c_str());
        return 1;
    }

    llama_context_params cp = llama_context_default_params();
    cp.n_ctx           = (uint32_t) n_ctx;
    cp.n_batch         = (uint32_t) r.n_batch;
    cp.n_threads       = n_threads;
    cp.n_threads_batch = n_threads;

    r.ctx = llama_init_from_model(r.model, cp);
    if (!r.ctx) {
        fprintf(stderr, "thalyx-engine: could not make a context of %d tokens\n", n_ctx);
        llama_model_free(r.model);
        return 1;
    }

    const uint64_t load_ms = now_ms() - started;

    {
        // The ready frame carries what a person needs to tell a cold answer
        // from a warm one, and the pid that makes "the same engine answered
        // both" checkable rather than asserted.
        std::vector<uint8_t> out;
        const char magic[4] = {'T', 'H', 'R', '1'};
        out.insert(out.end(), magic, magic + 4);
        put_u64(out, load_ms);
        put_u32(out, (uint32_t) getpid());
        put_u32(out, (uint32_t) n_threads);
        put_u32(out, (uint32_t) llama_n_ctx(r.ctx));
        if (!write_exactly(out.data(), out.size())) {
            return 1;
        }
    }

    for (;;) {
        char magic[4];
        if (!read_exactly(magic, 4)) {
            break;                              // Thalyx went away. So do we.
        }
        if (memcmp(magic, "THQ1", 4) != 0) {
            fprintf(stderr, "thalyx-engine: that is not a request frame\n");
            return 1;
        }

        uint32_t predict = 0;
        uint64_t seed    = 0;
        std::string prompt_path, grammar_path;
        if (!get_u32(predict) || !get_u64(seed) || !get_bytes(prompt_path) || !get_bytes(grammar_path)) {
            fprintf(stderr, "thalyx-engine: the request frame ended early\n");
            return 1;
        }

        const uint64_t began = now_ms();
        std::string prompt, grammar, answer, why;

        bool ok = slurp(prompt_path, prompt, why);
        if (ok && !grammar_path.empty()) {
            ok = slurp(grammar_path, grammar, why);
        }
        if (ok) {
            // Caught, because the weights took seconds to load and the things
            // that throw in here are things Thalyx handed over: a grammar file
            // that is not valid GBNF makes `common_sampler_init` throw, and an
            // engine that dies of it costs the *next* sentence a full reload
            // for a fault that was already reported. There is nothing to
            // recover inside the request — the answer is the reason it failed.
            try {
                ok = serve_one(r, prompt, grammar, predict, seed, answer, why);
            } catch (const std::exception & e) {
                ok = false;
                why = std::string("the engine could not run that: ") + e.what();
            }
        }

        // A failed request is answered, never fatal. The weights took seconds
        // to load and one bad path is not a reason to make the next sentence
        // pay for them again.
        if (!send_frame("THA1", ok ? 0 : 1, now_ms() - began, ok ? answer : why)) {
            break;
        }
    }

    llama_free(r.ctx);
    llama_model_free(r.model);
    llama_backend_free();
    return 0;
}
