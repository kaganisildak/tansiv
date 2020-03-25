# Copyright (C) 2018. The SimGrid Team. All rights reserved.

# This program is free software; you can redistribute it and/or modify
# it under the terms of the GNU Affero Licence (see in file LICENCE).

cmake_minimum_required(VERSION 2.8.10)
project (Tansiv C CXX)

set(CMAKE_CXX_FLAGS "${CMAKE_CXX_FLAGS} -std=c++11")
set(CMAKE_MODULE_PATH ${CMAKE_MODULE_PATH} "${CMAKE_SOURCE_DIR}/tools/cmake/")
# order of linked library is important !
# https://stackoverflow.com/questions/45135/why-does-the-order-in-which-libraries-are-linked-sometimes-cause-errors-in-gcc
set(LIBVSG_LIBS ${CMAKE_CURRENT_SOURCE_DIR}/src/fake-vm/target/debug/libfake_vm.a dl rt pthread cppunit)
set(VSG_INCLUDE_DIR ${CMAKE_CURRENT_SOURCE_DIR}/src/fake-vm/capi/src/)

# Search for SimGrid
find_package(SimGrid REQUIRED)

include_directories("${SimGrid_INCLUDE_DIR}" "${VSG_INCLUDE_DIR}"  SYSTEM)

# libvsg (rust implementation)
add_custom_target(vsg ALL COMMAND make WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}/src/fake-vm)

# Tansiv (coordinator of the simulation)
add_executable(tansiv src/simgrid/VmsInterface.cpp src/simgrid/VmsCoordinator.cpp)
target_link_libraries(tansiv ${SimGrid_LIBRARY} ${LIBVSG_LIBS})
target_include_directories(tansiv PUBLIC ${CMAKE_CURRENT_SOURCE_DIR}/src/include)

# Historical examples
add_executable(dummy_ping examples/dummy_ping/DummyPing.cpp)
target_link_libraries(dummy_ping PUBLIC ${LIBVSG_LIBS})
set_target_properties(dummy_ping PROPERTIES RUNTIME_OUTPUT_DIRECTORY examples/dummy_ping)

add_executable(dummy_pong examples/dummy_ping/DummyPong.cpp)
target_link_libraries(dummy_pong PUBLIC ${LIBVSG_LIBS})
set_target_properties(dummy_pong PROPERTIES RUNTIME_OUTPUT_DIRECTORY examples/dummy_ping)
configure_file(examples/dummy_ping/deployment.xml examples/dummy_ping/deployment.xml)

# VM based example. This also required a qemu with a modified slirp backend.
add_executable(sink examples/qemus/sink.c)
set_target_properties(sink PROPERTIES RUNTIME_OUTPUT_DIRECTORY examples/qemus)
target_link_libraries(sink PUBLIC vsg)
configure_file(examples/qemus/boot_and_log.sh examples/qemus/boot_and_log.sh)
configure_file(examples/qemus/deployment.xml examples/qemus/deployment.xml)

# A more testable example
add_executable(constant_rate examples/constant_rate/sink.c)
set_target_properties(constant_rate PROPERTIES RUNTIME_OUTPUT_DIRECTORY examples/constant_rate)
target_link_libraries(constant_rate PUBLIC ${LIBVSG_LIBS})
configure_file(examples/constant_rate/deployment.xml examples/constant_rate/deployment.xml)
configure_file(examples/constant_rate/nova_cluster.xml examples/constant_rate/nova_cluster.xml)
configure_file(examples/constant_rate/constant_rate.tesh examples/constant_rate/constant_rate.tesh)

# Unit tests
add_executable(tests src/tests/test.cpp)
target_link_libraries(tests PUBLIC ${LIBVSG_LIBS}  ${SimGrid_LIBRARY})

add_dependencies(tests vsg)

install(TARGETS tansiv DESTINATION bin)

