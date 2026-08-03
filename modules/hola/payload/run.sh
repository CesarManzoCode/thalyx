#!/bin/sh
# A module that exists to be looked at.
#
# It takes no arguments, changes nothing, and makes no claims. Every line is
# read from the process itself while it runs — and, deliberately, it never says
# whether it is confined. It cannot know, and a module that announced its own
# containment would be reciting a script rather than reporting a fact.
#
# The demonstration is the difference between two runs:
#
#     thalyx module run dev.thalyx.hola --unconfined
#     thalyx module run dev.thalyx.hola
#
# The first is what a program normally gets on your machine. The second is what
# Thalyx gives it. Nobody has to be told which is which.

count=0
for entry in $(ls / 2>/dev/null); do
    count=$((count + 1))
done

echo
echo "  Hola. Soy dev.thalyx.hola, y me instalaste tú."
echo
echo "  usuario   $(id -u)"
echo "  pid       $$"
echo "  raíz      $(pwd)"
echo
echo "  Desde aquí alcanzo a ver $count cosas en la raíz del sistema:"
for entry in $(ls / 2>/dev/null); do
    echo "    /$entry"
done
echo
echo "  No sé si eso es mucho o poco. Córreme de las dos formas y compara."
echo
