# Compile and install the various parts of the project:
# For now (10/21):
# - mkdir build && cd build && cmake -DCMAKE_INSTALL_PREFIX=/opt/tansiv .. && make
# - tansiv-clients: implement different client side flavors of the vsg protocol (rust)
#   + libtanproc.a and libtanqemu.a installed in /opt/tansiv-client
# - vsg: low-level simgrid side of the vsg protocol (c/c++)
#   + installed in $CMAKE_INSTALL_PREFIX
# - tansiv: simgrid simulation
#   + installed in $CMAKE_INSTALL_PREFIX

cmake_minimum_required(VERSION 2.8.10)
project (Tansiv C CXX)

set(CMAKE_CXX_FLAGS "${CMAKE_CXX_FLAGS} -std=c++14 -g")
set(CMAKE_MODULE_PATH ${CMAKE_MODULE_PATH} "${CMAKE_SOURCE_DIR}/tools/cmake/")
# order of linked library is important !
# https://stackoverflow.com/questions/45135/why-does-the-order-in-which-libraries-are-linked-sometimes-cause-errors-in-gcc
set(TANPROC_LIBS /opt/tansiv-client/lib/libtanproc.a dl rt pthread)
set(TANSIVCLIENT_INCLUDE_DIR /opt/tansiv-client/include)
set(VSG_INCLUDE_DIR ${CMAKE_CURRENT_SOURCE_DIR}/src/vsg/)

# Search for SimGrid
find_package(SimGrid REQUIRED)

include_directories("${SimGrid_INCLUDE_DIR}" "${TANSIVCLIENT_INCLUDE_DIR}" "${VSG_INCLUDE_DIR}"  SYSTEM)

# To build tanqemu we need to generate the config-host.h header
add_custom_target(config-host.h COMMAND ./configure --target-list=x86_64-softmmu && make -j config-host.h WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}/src/qemu)
# tansiv-client client libs tanproc and tanqemu (Rust implementation exporting a C API)
# installed in /opt/tansiv-client
# compiled in RELEASE mode (I've got the power !)
add_custom_target(tansiv-clients ALL COMMAND make PREFIX=/opt/tansiv-client RELEASE=0 install WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}/src/client)
add_dependencies(tansiv-clients config-host.h)

# build the qemu base image
add_custom_target(packer COMMAND packer build debian-10.3.0-x86_64.json WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}/packer)

# vsg lib (actor side)
add_library(vsg STATIC src/vsg/vsg.c src/vsg/log.c)
target_compile_options(vsg PUBLIC -DLOG_USE_COLOR)
target_link_libraries(vsg PUBLIC m)

# Tansiv (coordinator of the simulation)
add_executable(tansiv src/simgrid/VmsInterface.cpp src/simgrid/VmsCoordinator.cpp)
target_link_libraries(tansiv ${SimGrid_LIBRARY} ${TANPROC_LIBS} vsg)
target_include_directories(tansiv PUBLIC ${CMAKE_CURRENT_SOURCE_DIR}/src/include)

# Example with tanproc and simgrid
add_executable(send examples/send/send.cpp)
set_target_properties(send PROPERTIES RUNTIME_OUTPUT_DIRECTORY examples/send)
target_link_libraries(send PUBLIC ${TANPROC_LIBS} vsg)
configure_file(examples/send/deployment.xml examples/send/deployment.xml)
configure_file(examples/send/nova_cluster.xml examples/send/nova_cluster.xml)
add_dependencies(send tansiv-clients)

# Benchs
add_executable(gettimeofday examples/benchs/gettimeofday.cpp)
set_target_properties(gettimeofday PROPERTIES RUNTIME_OUTPUT_DIRECTORY examples/benchs)
target_link_libraries(gettimeofday PUBLIC ${TANPROC_LIBS} vsg)
configure_file(examples/benchs/deployment.xml examples/benchs/deployment.xml)
configure_file(examples/benchs/nova_cluster.xml examples/benchs/nova_cluster.xml)
add_dependencies(gettimeofday tansiv-clients)

# Unit tests
add_executable(tests src/tests/tests.cpp src/tests/scenario.cpp)
target_include_directories(tests PUBLIC ${CMAKE_CURRENT_SOURCE_DIR}/src/include)
target_link_libraries(tests PUBLIC ${TANPROC_LIBS} vsg ${SimGrid_LIBRARY})

add_dependencies(tests tansiv-clients)

install(TARGETS tansiv DESTINATION bin)
install(TARGETS vsg DESTINATION lib)
install(FILES src/vsg/vsg.h DESTINATION include)
install(CODE "MESSAGE(\"-- Installing tansiv client libraries\")")
install(CODE "execute_process(COMMAND make PREFIX=/opt/tansiv-client RELEASE=0 install WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}/src/client )")
