#!/usr/bin/env python3
"""A model small enough to be built here and real enough for llama.cpp to run.

`dev/engine-needs.sh` measures which syscalls an inference engine makes. That
needs an engine *working* — loading weights, starting its thread pool,
tokenizing, running the graph — and working needs a model.

Written with llama.cpp's own `gguf-py` rather than by hand, which is rule 6 of
`CLAUDE.md` turned around: a file this script's author invented would prove that
llama.cpp accepts what its author believes GGUF to be. Here the writer is the
tool's own, and the reader is the tool itself, so the only opinion in the loop
belongs to llama.cpp.

Two layers and 64 dimensions on purpose. The question is *which* calls an engine
makes, and a two-layer model makes the same ones as a seventy-billion-parameter
one. It answers nothing about size; `engine-needs.sh` says so where it prints.
"""

import sys
import pathlib
import numpy as np

if len(sys.argv) != 3:
    sys.exit("usage: tiny-model.py <llama.cpp checkout> <output.gguf>")

source, out = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
sys.path.insert(0, str(source / "gguf-py"))
import gguf  # noqa: E402  — the path has to be set first

DIM, LAYERS, HEADS, KV_HEADS, FEED_FORWARD = 64, 2, 4, 4, 128

writer = gguf.GGUFWriter(str(out), "llama")
writer.add_context_length(128)
writer.add_embedding_length(DIM)
writer.add_block_count(LAYERS)
writer.add_feed_forward_length(FEED_FORWARD)
writer.add_head_count(HEADS)
writer.add_head_count_kv(KV_HEADS)
writer.add_rope_dimension_count(DIM // HEADS)
writer.add_layer_norm_rms_eps(1e-5)
writer.add_file_type(gguf.LlamaFileType.ALL_F32)

# The 256 byte tokens are not padding. An SPM vocabulary finds the newline by
# looking up the byte token <0x0A>, and a vocabulary without them loads with a
# warning and then dies inside `unordered_map::at` on the first tokenize —
# found by running it, which is the only way any of this is ever found.
tokens = (
    ["<unk>", "<s>", "</s>"]
    + [f"<0x{byte:02X}>" for byte in range(256)]
    + ["hola", "▁hola"]
)
kinds = (
    [gguf.TokenType.CONTROL] * 3
    + [gguf.TokenType.BYTE] * 256
    + [gguf.TokenType.NORMAL] * 2
)
writer.add_tokenizer_model("llama")
writer.add_token_list(tokens)
writer.add_token_scores([0.0] * len(tokens))
writer.add_token_types(kinds)
writer.add_bos_token_id(1)
writer.add_eos_token_id(2)
writer.add_unk_token_id(0)

# Seeded, so two runs of this script produce the same file byte for byte. A
# measurement whose input changed every time could not be compared with itself.
noise = np.random.default_rng(0)


def weights(*shape):
    return (noise.standard_normal(shape) * 0.02).astype(np.float32)


vocab = len(tokens)
writer.add_tensor("token_embd.weight", weights(vocab, DIM))
writer.add_tensor("output_norm.weight", np.ones(DIM, np.float32))
writer.add_tensor("output.weight", weights(vocab, DIM))
for layer in range(LAYERS):
    at = f"blk.{layer}."
    writer.add_tensor(at + "attn_norm.weight", np.ones(DIM, np.float32))
    writer.add_tensor(at + "attn_q.weight", weights(DIM, DIM))
    writer.add_tensor(at + "attn_k.weight", weights(DIM // HEADS * KV_HEADS, DIM))
    writer.add_tensor(at + "attn_v.weight", weights(DIM // HEADS * KV_HEADS, DIM))
    writer.add_tensor(at + "attn_output.weight", weights(DIM, DIM))
    writer.add_tensor(at + "ffn_norm.weight", np.ones(DIM, np.float32))
    writer.add_tensor(at + "ffn_gate.weight", weights(FEED_FORWARD, DIM))
    writer.add_tensor(at + "ffn_up.weight", weights(FEED_FORWARD, DIM))
    writer.add_tensor(at + "ffn_down.weight", weights(DIM, FEED_FORWARD))

writer.write_header_to_file()
writer.write_kv_data_to_file()
writer.write_tensors_to_file()
writer.close()
print(f"{out.name}: {out.stat().st_size} bytes, {LAYERS} layers, {vocab} tokens")
