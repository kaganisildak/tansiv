# Copyright (C) 2018. The SimGrid Team. All rights reserved.

# This program is free software; you can redistribute it and/or modify
# it under the terms of the GNU Affero Licence (see in file LICENCE).

cmake_minimum_required(VERSION 2.8.10)
project (SimGrid-FMI CXX)

set(CMAKE_CXX_FLAGS "${CMAKE_CXX_FLAGS} -std=c++11")
set(CMAKE_MODULE_PATH ${CMAKE_MODULE_PATH} "${CMAKE_SOURCE_DIR}/tools/cmake/")

# Search for SimGrid
find_package(SimGrid REQUIRED)

include_directories("${SimGrid_INCLUDE_DIR}" SYSTEM)

include_directories(src/include)

set(SOURCE_FILES
        src/simgrid/VmsInterface.cpp
  )

#enable_testing()

# Enable Testing
# include(${CMAKE_HOME_DIRECTORY}/tools/cmake/Tests.cmake)
# add_subdirectory(${PROJECT_SOURCE_DIR}/examples)

# Install everything

add_executable(vsg ${SOURCE_FILES} src/simgrid/VmsCoordinator.cpp)
add_executable(dummy_ping examples/dummy_ping/DummyPing.cpp)
add_executable(dummy_pong examples/dummy_ping/DummyPong.cpp)
set_target_properties(dummy_ping PROPERTIES RUNTIME_OUTPUT_DIRECTORY examples/dummy_ping)
set_target_properties(dummy_pong PROPERTIES RUNTIME_OUTPUT_DIRECTORY examples/dummy_ping)

target_link_libraries(vsg ${SimGrid_LIBRARY})

install(TARGETS vsg DESTINATION bin)


