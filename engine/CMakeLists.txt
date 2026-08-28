# Built inside llama.cpp's own tree, on purpose.
#
# `dev/build-engine.sh` copies this directory into the pinned checkout's
# `tools/` and appends one `add_subdirectory` line. That is cheaper and safer
# than a second CMake project that links the static archives by hand: link
# order across ggml's backends is fiddly, it changes between tags, and getting
# it wrong produces an engine that fails at link time on somebody else's
# machine. Here the engine is built with exactly the flags `llama-completion`
# is built with, because it is built by the same configure.

set(TARGET thalyx-engine)

add_executable(${TARGET} thalyx-engine.cpp)
target_link_libraries(${TARGET} PRIVATE llama-common llama ${CMAKE_THREAD_LIBS_INIT})
target_compile_features(${TARGET} PRIVATE cxx_std_17)
