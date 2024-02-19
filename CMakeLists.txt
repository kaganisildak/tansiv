# Compile and install the various parts of the project:
# For now (10/21):
# - mkdir build && cd build && cmake -DFLATBUFFERS_SRC=$(pwd)/../../flatbuffers/flatbuffers -DCMAKE_INSTALL_PREFIX=/opt/tansiv .. && make
# - tansiv-clients: implement different client side flavors of the vsg protocol (rust)
#   + libtanproc.a, libtanqemu.a and libtanqemukvm.a installed in /opt/tansiv-client
# - vsg: low-level simgrid side of the vsg protocol (c/c++)
#   + installed in $CMAKE_INSTALL_PREFIX
# - tansiv: simgrid simulation
#   + installed in $CMAKE_INSTALL_PREFIX

cmake_minimum_required(VERSION 3.5)
project (Tansiv C CXX)

set(CMAKE_CXX_FLAGS "${CMAKE_CXX_FLAGS} -std=c++17 -g")
set(CMAKE_MODULE_PATH ${CMAKE_MODULE_PATH} "${CMAKE_SOURCE_DIR}/tools/cmake/")

# use this to link against tanproc
# order of linked library is important !
# https://stackoverflow.com/questions/45135/why-does-the-order-in-which-libraries-are-linked-sometimes-cause-errors-in-gcc
set(TANPROC_LIB ${CMAKE_INSTALL_PREFIX}/lib/libtanproc.a)
set(TANPROC_EXTRA_LIBS ${TANPROC_LIB} dl rt pthread)
# use this to link against tanqemu(kvm) (obvly only qemu needs that)
set(TANQEMU_LIB ${CMAKE_INSTALL_PREFIX}/lib/libtanqemu.a)
set(TANQEMUKVM_LIB ${CMAKE_INSTALL_PREFIX}/lib/libtanqemukvm.a)
set(TANXEN_LIB ${CMAKE_INSTALL_PREFIX}/lib/libtanxen.a)
set(TANSIVTIMER_LIB ${CMAKE_INSTALL_PREFIX}/lib/libtansiv-timer.a)
set(TANSIVTIMERXEN_LIB ${CMAKE_INSTALL_PREFIX}/lib/libtansiv-timer-xen.a)

set(TANSIVCLIENT_INCLUDE_DIR ${CMAKE_INSTALL_PREFIX}/include)
set(WIRE_INCLUDE_DIR ${CMAKE_CURRENT_SOURCE_DIR}/src/wire)
set(CLIENT_SOURCE_DIR ${CMAKE_CURRENT_SOURCE_DIR}/src/client/)
set(XEN_TANSIV_BRIDGE_SOURCE_DIR ${CMAKE_CURRENT_SOURCE_DIR}/src/xen/)

find_package(FlatBuffers REQUIRED)
# This compile a schema to cpp
# - This makes the target fbs_packets available to other make tasks
# - This adds the include output dir to CMAKE's include directory
build_flatbuffers("${WIRE_INCLUDE_DIR}/packets.fbs" "" "fbs_packets" "" "${WIRE_INCLUDE_DIR}/" "" "")

# Search for SimGrid
find_package(SimGrid REQUIRED)
find_package(tinyxml2 REQUIRED)

# Search for ns3
find_package(NS3 REQUIRED)

include_directories("${SimGrid_INCLUDE_DIR}" "${TANSIVCLIENT_INCLUDE_DIR}" "${WIRE_INCLUDE_DIR}"  SYSTEM)
include_directories("${NS3_INCLUDE_DIR}" "${TANSIVCLIENT_INCLUDE_DIR}" "${WIRE_INCLUDE_DIR}"  SYSTEM)

# To build tanqemu we need to generate the config-host.h header
# as part of 7.2.0 the file is generated at configure phase
# TODO: use prefix
add_custom_target(config-host.h COMMAND ./configure --target-list=x86_64-softmmu && cd build && ninja config-poison.h
                                WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}/src/qemu)
# tansiv-client client libs tanproc tanqemu and tanqemukvm (Rust implementation exporting a C API)
# installed in /opt/tansiv-client
# compiled in RELEASE mode (I've got the power !)
# build the qemu base image
# - installed in /opt/tansiv/
# - compiled in RELEASE mode (I've got the power !)
# - making flatc known to the cargo step (this is required to compile the flatbuffers schema to rust)
add_custom_target(tansiv-clients COMMAND make PREFIX=${CMAKE_INSTALL_PREFIX} RELEASE=0 install
                                 WORKING_DIRECTORY ${CLIENT_SOURCE_DIR})
add_dependencies(tansiv-clients config-host.h)

# tansiv-timer lib (tanqemukvm)
add_library(tansiv-timer STATIC src/kvm/tansiv-timer/tansiv-timer.c)
target_compile_options(tansiv-timer PUBLIC -fPIE)

# tansiv-timer-xen lib (tanxen)
add_library(tansiv-timer-xen STATIC src/xen/tansiv-timer/tansiv-timer-xen.c)
target_compile_options(tansiv-timer-xen PUBLIC -fPIE)

# Tansiv (coordinator of the simulation)
#link_directories(${NS3_LIBRARY_PATH})

add_executable(tansiv-simgrid src/coordinator/VmsInterface.cpp src/coordinator/VmsCoordinator-simgrid.cpp ${WIRE_INCLUDE_DIR}/socket.cpp)
add_executable(tansiv-ns3 src/coordinator/VmsInterface.cpp src/coordinator/VmsCoordinator-ns3.cpp ${WIRE_INCLUDE_DIR}/socket.cpp)

add_custom_target(xen_tansiv_bridge ALL COMMAND make WORKING_DIRECTORY ${XEN_TANSIV_BRIDGE_SOURCE_DIR})

target_link_libraries(tansiv-ns3 tinyxml2)
string(STRIP ${NS3_LIBRARIES} NS3_LIBRARIES)
message(STATUS ${NS3_LIBRARIES})
message(STATUS ${NS3_LIBRARY_PATH})

target_link_libraries(tansiv-simgrid ${SimGrid_LIBRARY} flatbuffers)
target_include_directories(tansiv-simgrid PUBLIC ${WIRE_INCLUDE_DIR} fbs_packets)
add_dependencies(tansiv-simgrid fbs_packets)

target_link_libraries(tansiv-ns3 ${NS3_LIBRARIES} flatbuffers)
target_include_directories(tansiv-ns3 PUBLIC ${WIRE_INCLUDE_DIR} fbs_packets)
add_dependencies(tansiv-ns3 fbs_packets)

# Example with tanproc and tansiv
add_executable(send examples/send/send.cpp)
set_target_properties(send PROPERTIES RUNTIME_OUTPUT_DIRECTORY examples/send)
target_link_libraries(send PUBLIC ${TANPROC_EXTRA_LIBS})
configure_file(examples/send/deployment.xml examples/send/deployment.xml)
configure_file(examples/send/platform.xml examples/send/platform.xml)
add_dependencies(send tansiv-clients)

# Benchs
add_executable(gettimeofday examples/benchs/gettimeofday.cpp)
set_target_properties(gettimeofday PROPERTIES RUNTIME_OUTPUT_DIRECTORY examples/benchs)
target_link_libraries(gettimeofday PUBLIC ${TANPROC_EXTRA_LIBS})
configure_file(examples/benchs/deployment.xml examples/benchs/deployment.xml)
configure_file(examples/benchs/nova_cluster.xml examples/benchs/nova_cluster.xml)
add_dependencies(gettimeofday tansiv-clients)

# "Unit" tests
add_executable(tests src/tests/tests.cpp src/tests/scenario.cpp ${WIRE_INCLUDE_DIR}/socket.cpp)
target_link_libraries(tests PUBLIC ${TANPROC_EXTRA_LIBS} flatbuffers)
configure_file(src/tests/tests.sh tests.sh)
add_dependencies(tests tansiv-clients fbs_packets)

# Launch tests
add_custom_target(run-tansiv-tests COMMAND  ./tests.sh)
add_dependencies(run-tansiv-tests tests)
add_custom_target(run-client-tests COMMAND PREFIX=${CMAKE_INSTALL_PREFIX} RELEASE=0 make test
                                   WORKING_DIRECTORY ${CLIENT_SOURCE_DIR})
## launch all tests
add_custom_target(run-tests)
add_dependencies(run-tests run-tansiv-tests run-client-tests)

install(TARGETS tansiv-ns3 DESTINATION bin)
install(TARGETS tansiv-simgrid DESTINATION bin)
install(TARGETS tansiv-timer DESTINATION lib)
install(TARGETS tansiv-timer-xen DESTINATION lib)
install(FILES src/kvm/tansiv-timer/tansiv-timer.h DESTINATION include)
install(FILES src/xen/tansiv-timer/tansiv-timer-xen.h DESTINATION include)
install(PROGRAMS bin/notansiv.py DESTINATION bin)
install(DIRECTORY bin/templates DESTINATION bin)
install(PROGRAMS bin/boot.py DESTINATION bin)
install(PROGRAMS src/xen/xen_tansiv_bridge DESTINATION bin)
install(CODE "MESSAGE(\"-- Installing tansiv client libraries\")")
install(CODE "execute_process(COMMAND make PREFIX=${CMAKE_INSTALL_PREFIX} RELEASE=0 install WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}/src/client )")
