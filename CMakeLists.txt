# Copyright (C) 2018. The SimGrid Team. All rights reserved.

# This program is free software; you can redistribute it and/or modify
# it under the terms of the GNU Affero Licence (see in file LICENCE).

cmake_minimum_required(VERSION 2.8.10)
project (Tansiv C CXX)

set(CMAKE_CXX_FLAGS "${CMAKE_CXX_FLAGS} -std=c++11")
set(CMAKE_MODULE_PATH ${CMAKE_MODULE_PATH} "${CMAKE_SOURCE_DIR}/tools/cmake/")
# order of linked library is important !
# https://stackoverflow.com/questions/45135/why-does-the-order-in-which-libraries-are-linked-sometimes-cause-errors-in-gcc
set(FAKEVM_LIBS ${CMAKE_CURRENT_SOURCE_DIR}/src/fake-vm/target/debug/libfake_vm.a dl rt pthread cppunit)
set(FAKEVM_INCLUDE_DIR ${CMAKE_CURRENT_SOURCE_DIR}/src/fake-vm/capi/src/)
set(VSG_INCLUDE_DIR ${CMAKE_CURRENT_SOURCE_DIR}/src/vsg/)

# Search for SimGrid
find_package(SimGrid REQUIRED)

include_directories("${SimGrid_INCLUDE_DIR}" "${FAKEVM_INCLUDE_DIR}" "${VSG_INCLUDE_DIR}"  SYSTEM)

# fake-vm client lib (rust implementation)
add_custom_target(fake-vm ALL COMMAND make WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}/src/fake-vm)

# vsg lib (actor side)
add_library(vsg STATIC src/vsg/vsg.c src/vsg/log.c)
target_compile_options(vsg PUBLIC -DLOG_USE_COLOR)
target_link_libraries(vsg PUBLIC m)


# Tansiv (coordinator of the simulation)
add_executable(tansiv src/simgrid/VmsInterface.cpp src/simgrid/VmsCoordinator.cpp)
target_link_libraries(tansiv ${SimGrid_LIBRARY} ${FAKEVM_LIBS} vsg)
target_include_directories(tansiv PUBLIC ${CMAKE_CURRENT_SOURCE_DIR}/src/include)

# Example with fake-vm and simgrid
add_executable(send examples/send/send.cpp)
set_target_properties(send PROPERTIES RUNTIME_OUTPUT_DIRECTORY examples/send)
target_link_libraries(send PUBLIC ${FAKEVM_LIBS} vsg)
configure_file(examples/send/deployment.xml examples/send/deployment.xml)
configure_file(examples/send/nova_cluster.xml examples/send/nova_cluster.xml)
add_dependencies(send fake-vm)

# Benchs
add_executable(gettimeofday examples/benchs/gettimeofday.cpp)
set_target_properties(gettimeofday PROPERTIES RUNTIME_OUTPUT_DIRECTORY examples/benchs)
target_link_libraries(gettimeofday PUBLIC ${FAKEVM_LIBS} vsg)
configure_file(examples/benchs/deployment.xml examples/benchs/deployment.xml)
configure_file(examples/benchs/nova_cluster.xml examples/benchs/nova_cluster.xml)
add_dependencies(gettimeofday fake-vm)

# Qemus
configure_file(examples/qemus/boot_and_log.sh examples/qemus/boot_and_log.sh COPYONLY)
configure_file(examples/qemus/deployment.xml examples/qemus/deployment.xml COPYONLY)
configure_file(examples/qemus/nova_cluster.xml examples/qemus/nova_cluster.xml COPYONLY)

# Unit tests
add_executable(tests src/tests/tests.cpp)
target_link_libraries(tests PUBLIC ${FAKEVM_LIBS} vsg ${SimGrid_LIBRARY})
target_compile_options(tests PUBLIC -fexceptions)

add_dependencies(tests fake-vm)

install(TARGETS tansiv DESTINATION bin)
install(TARGETS vsg DESTINATION lib)
install(FILES src/vsg/vsg.h DESTINATION include)
install(CODE "MESSAGE(\"-- Installing libfake_vm\")")
install(CODE "execute_process(COMMAND make install WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}/src/fake-vm )")