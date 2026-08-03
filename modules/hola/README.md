# `dev.thalyx.hola`

A module that exists to be looked at.

It has no function. That is deliberate: the question it answers is not "what can
a module do for me" but the one that comes before it — **what is a module, and
what does installing one actually mean?**

It takes no arguments, changes nothing, asks for no permissions, and makes no
claims. It prints what the process can see from inside itself while it runs.

## The demonstration is two runs, not one

```sh
thalyx module run dev.thalyx.hola --unconfined   # what a program normally gets
thalyx module run dev.thalyx.hola                # what Thalyx gives it
```

The module never says which of those it is in — it cannot know, and a module
that announced its own containment would be reciting a script rather than
reporting a fact. The first run lists your machine. The second lists what it was
given. Nobody has to be told which is which.

An earlier version of this script ended with "there is no further" — a sentence
that is true confined and a lie unconfined. It was caught by running it both
ways. A module that asserts its own confinement is exactly the theatre this
project refuses, and it is easy to write by accident.

## Building it

```sh
thalyx dev keygen --out key.hex
thalyx dev pack modules/hola/payload \
    --manifest modules/hola/manifest.toml \
    --key key.hex --out hola.thmod
```
