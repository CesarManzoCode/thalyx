# Working in a Thalyx machine from your editor

Thalyx can be the machine a programming agent works **in**, while the agent
itself stays on your host. Claude Code, VS Code and any other MCP client load
the same server and see the same tools.

The decree behind this is `vault/07-Adopcion-y-Fases/Agentes-Externos.md`, and
the short version of it is: **MCP is an adapter.** Thalyx's own surface is the
authority; nothing on the host holds a filesystem, a graph or a rollback.

---

## The whole thing, in three commands

```sh
make -C image agent PROJECT=~/code/my-project    # imports, builds the disk, boots
dev/agent-connect.sh                             # in another terminal, once it is up
claude                                           # and ask it something
```

That is it. `/mcp` inside Claude Code lists what it can see.

The first command asks for `sudo` once, out loud, for the one step that needs it
— building the store means formatting a loop device and mounting it. Nothing
else runs as root.

---

## What each step actually does

### `make -C image agent PROJECT=<dir>`

1. Copies your project onto the store's staging area. **A copy** — your checkout
   is never touched and is not reachable from inside the machine. `.git` comes
   along so that `git diff` on the host has something to diff against later.
2. Builds the store disk, with the project as **its own Btrfs subvolume** under
   `/home`. That is not tidiness: `intento` snapshots exactly the subvolume the
   session stands in, so a workspace that were a plain directory would make
   every `thalyx_attempt` answer `not_a_subvolume`.
3. Boots QEMU with the ordinary screen **plus** one extra device:

   ```
   -device virtio-serial-pci
   -chardev socket,path=image/build/agent.sock,server=on,wait=off,id=thalyxagent
   -device virtserialport,chardev=thalyxagent,name=org.thalyx.agent
   ```

   and `thalyx.workspace=/home/<project>` on the kernel command line.

No network, no port forward, no `-netdev`. The machine is reachable without ever
having an address.

`make -C image run` is untouched: a machine booted that way has no channel, and
cannot tell that any of this exists — no error, no wait, no line on the boot
report.

### `dev/agent-connect.sh [socket]`

1. Builds `thalyx-mcp` **for your host** (not the static musl build that goes
   inside the image).
2. Connects and reads the machine's hello, before registering anything. A client
   configured against a machine that is not answering fails on the model's first
   tool call, which reads as the model's mistake.
3. Registers the server with Claude Code (`claude mcp add thalyx --scope local`)
   and writes `.vscode/mcp.json` beside it.

If you would rather do it by hand:

```sh
claude mcp add thalyx -- /path/to/thalyx/target/release/thalyx-mcp \
    --connect /path/to/thalyx/image/build/agent.sock
```

### VS Code, and other MCP clients

`.vscode/mcp.json`, which `agent-connect.sh` writes:

```json
{
  "servers": {
    "thalyx": {
      "type": "stdio",
      "command": "/path/to/thalyx/target/release/thalyx-mcp",
      "args": ["--connect", "/path/to/thalyx/image/build/agent.sock"]
    }
  }
}
```

Same binary, same tools. There is deliberately **no** second integration per
client: MCP is the boundary. A Codex or Copilot client that speaks MCP uses the
same two lines.

---

## The tools

Eleven, and the number is a decision. Every tool an agent is shown is a branch
it considers on every turn, so the question asked of each was not *does this
verb exist* but *can this make an agent program better*.

| tool | what it is for |
|---|---|
| `thalyx_state` | what this machine is, in one call |
| `thalyx_list` | what is in a directory |
| `thalyx_read` | one file, with its exact size and sha256 |
| `thalyx_index` | read the tree and record what refers to what |
| `thalyx_symbol` | where a name is defined and every place it is used |
| `thalyx_dependencies` | impact, without opening any file |
| `thalyx_find` | name patterns, or literal text — the fallback |
| `thalyx_edit` | change a file by line; every answer carries its undo |
| `thalyx_file` | create, delete, move, copy |
| `thalyx_attempt` | begin / commit / abandon a reversible boundary |
| `thalyx_changed` | what changed since the checkpoint |

---

## What an agent cannot do

Being connected to the port is not authority.

- **One workspace, and it cannot leave it.** Every path is resolved twice — the
  way the verb resolves it, and the way the *kernel* does, with symlinks
  followed. Both have to land inside. A `..` is refused outright, because the
  two ways of folding it give different files the moment a symlink is involved.
- **The verbs are named one by one.** `apagar`, `instalar-en`, `negar`,
  `observar`, `correr`, `ejecutar` and `matar` are not reachable, and neither is
  rehearsing them.
- **Nothing it does is anonymous.** Every change it makes, and every attempt to
  leave the workspace, lands in the journal with `operation: external_agent` and
  `origin: untrusted_content`. `historia` inside the machine shows them.

---

## Getting the work back out

```sh
sudo make -C image agent-export INTO=~/code/my-project-after
diff -ru ~/code/my-project ~/code/my-project-after
```

A copy again, into a directory that must not already exist. Nothing writes over
your original, ever.

---

## Comparing it against ordinary tools

```sh
dev/bench-external-agent.sh --project ~/code/my-project --symbol SomeType --task read \
    --expect-file ~/code/my-project.expected
```

Two arms, the same prompt, the same model, the same turn limit: Claude Code with
its usual tools on a host copy, and the same Claude Code with **only** the
Thalyx tools on an identical copy inside the machine. Arm B has its ordinary
tools taken away on purpose — left in, the model reaches for what it has used a
billion times and the run measures nothing.

It writes `summary.json` with, for **both** arms: turns, wall time, whatever
token counts and cost Claude Code itself reports, every tool called by name, how
many bytes each handed back to the model, files read and text searches. A field
the agent did not print is **absent**, never zero — the summary never carries a
number nobody measured.

`--expect-file` is optional and is what turns the run into a pass or a fail: one
string per line that the final answer has to contain, written by hand from what
you already know is true about the project — the file where the symbol is
defined, and every file that really depends on it. Without it the summary
reports no verdict at all rather than a guessed one, because an agent that
answers confidently and wrongly must not score as a success. Lines starting with
`#` are comments.

```
src/store.rs
src/handler.rs
src/server.rs
```

**One run of one task is an anecdote.** What the script gives you is the ability
to run the comparison at all.

Every run that has actually been made is written down, one by one — including
the one where Thalyx showed no advantage — in
`vault/07-Adopcion-y-Fases/Evidencia-de-Agentes.md`. That note is the canonical
record: numbers there are never averaged across runs, and a run that did not
favour Thalyx is written up in the same detail as one that did.

---

## When something is wrong

**`no Thalyx machine at …`** — the VM is not up, or was booted with `make run`
rather than `make run-agent`. `thalyx-mcp` waits 30 seconds by default, so
starting it while the machine boots is fine.

**`thalyx-mcp: thalyx_symbol is not offered`** — the machine does not have that
verb. A tool whose verbs the machine did not advertise is dropped rather than
offered, so a version skew is visible on stderr instead of looking like a model
that chose not to use the tool.

**`not_a_subvolume` from `thalyx_attempt`** — the workspace is a plain
directory. That happens if it was put on the store by hand instead of by
`make -C image agent`.

**The machine came up and says `no org.thalyx.agent port`** — QEMU was started
without the virtio-serial device, or the kernel was built without
`CONFIG_VIRTIO_CONSOLE`.
