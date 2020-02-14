# Copyright (C) 2018. The SimGrid Team. All rights reserved.

# This program is free software; you can redistribute it and/or modify
# it under the terms of the GNU Affero Licence (see in file LICENCE).

cmake_minimum_required(VERSION 2.8.10)
project (Tansiv C CXX)

set(CMAKE_CXX_FLAGS "${CMAKE_CXX_FLAGS} -std=c++11")
set(CMAKE_MODULE_PATH ${CMAKE_MODULE_PATH} "${CMAKE_SOURCE_DIR}/tools/cmake/")

# Search for SimGrid
find_package(SimGrid REQUIRED)

include_directories("${SimGrid_INCLUDE_DIR}" SYSTEM)

# include_directories(src/include
#                    src/vsg)

# set(SOURCE_FILES
#    src/simgrid/VmsInterface.cpp
#  )

add_subdirectory(src/vsg)

#enable_testing()

# Enable Testing
# include(${CMAKE_HOME_DIRECTORY}/tools/cmake/Tests.cmake)
# add_subdirectory(${PROJECT_SOURCE_DIR}/examples)

# Install everything


add_executable(tansiv src/simgrid/VmsInterface.cpp src/simgrid/VmsCoordinator.cpp)
target_link_libraries(tansiv ${SimGrid_LIBRARY} vsg)
target_include_directories(tansiv PUBLIC ${CMAKE_CURRENT_SOURCE_DIR}/src/include)

add_executable(dummy_ping examples/dummy_ping/DummyPing.cpp)
target_link_libraries(dummy_ping PUBLIC vsg)
set_target_properties(dummy_ping PROPERTIES RUNTIME_OUTPUT_DIRECTORY examples/dummy_ping)
add_executable(dummy_pong examples/dummy_ping/DummyPong.cpp)
target_link_libraries(dummy_pong PUBLIC vsg)
set_target_properties(dummy_pong PROPERTIES RUNTIME_OUTPUT_DIRECTORY examples/dummy_ping)
configure_file(examples/dummy_ping/deployment.xml examples/dummy_ping/deployment.xml)

add_executable(sink examples/qemu_sink/sink.c)
set_target_properties(sink PROPERTIES RUNTIME_OUTPUT_DIRECTORY examples/qemu_sink)
target_link_libraries(sink PUBLIC vsg)
configure_file(examples/qemu_sink/boot_and_log.sh examples/qemu_sink/boot_and_log.sh)
configure_file(examples/qemu_sink/deployment.xml examples/qemu_sink/deployment.xml)

install(TARGETS tansiv DESTINATION bin)

