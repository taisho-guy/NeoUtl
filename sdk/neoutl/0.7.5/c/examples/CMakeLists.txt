cmake_minimum_required(VERSION 3.16)
project(neoutl_c_samples C)

set(CMAKE_C_STANDARD 99)
set(CMAKE_C_STANDARD_REQUIRED ON)

include_directories(${CMAKE_CURRENT_SOURCE_DIR}/../include)

# Effect sample
add_library(sample_effect SHARED sample_effect.c)
set_target_properties(sample_effect PROPERTIES PREFIX "lib")

# Object sample
add_library(sample_object SHARED sample_object.c)
set_target_properties(sample_object PROPERTIES PREFIX "lib")

# Extension sample
add_library(sample_extension SHARED sample_extension.c)
set_target_properties(sample_extension PROPERTIES PREFIX "lib")
