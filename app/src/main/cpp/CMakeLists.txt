cmake_minimum_required(VERSION 3.22)
project(voicio_llm C CXX)
set(CMAKE_CXX_STANDARD 17)

# llama.cpp, vendored by scripts/vendor-llama-cpp.sh. Only the library is built.
set(LLAMA_BUILD_COMMON   OFF CACHE BOOL "" FORCE)
set(LLAMA_BUILD_TESTS    OFF CACHE BOOL "" FORCE)
set(LLAMA_BUILD_TOOLS    OFF CACHE BOOL "" FORCE)
set(LLAMA_BUILD_EXAMPLES OFF CACHE BOOL "" FORCE)
set(LLAMA_BUILD_SERVER   OFF CACHE BOOL "" FORCE)
set(LLAMA_BUILD_APP      OFF CACHE BOOL "" FORCE)
set(LLAMA_CURL           OFF CACHE BOOL "" FORCE)
set(LLAMA_OPENSSL        OFF CACHE BOOL "" FORCE)
set(GGML_OPENMP          OFF CACHE BOOL "" FORCE)
# One CPU backend per instruction-set level, picked at run time: new phones get the fast dot-product
# and int8 matrix kernels, older phones still run (no illegal-instruction crash).
set(BUILD_SHARED_LIBS    ON  CACHE BOOL "" FORCE)
set(GGML_BACKEND_DL      ON  CACHE BOOL "" FORCE)
set(GGML_CPU_ALL_VARIANTS ON CACHE BOOL "" FORCE)
set(GGML_NATIVE          OFF CACHE BOOL "" FORCE)
add_subdirectory(llama.cpp llama)

add_library(voicio_llm SHARED llm_jni.cpp)
target_include_directories(voicio_llm PRIVATE llama.cpp/include llama.cpp/ggml/include)
target_link_libraries(voicio_llm PRIVATE llama log)
