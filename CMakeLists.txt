# Compile and install the various par of the project:
# For now (09/20):
# - make DESTDIR=/opt/tansiv
# - fake-vm: implements the client side of the vsg protocol (rust)
#   + installed in  /opt/fake-vm
# - vsg: implements the simgrid side of the vsg protocol (c/c++)
#   + installed in $DESTDIR
# - tansiv: simgrid simulation
#   + installed in $DESTDIR

cmake_minimum_required(VERSION 2.8.10)
project (Tansiv C CXX)

set(CMAKE_CXX_FLAGS "${CMAKE_CXX_FLAGS} -std=c++11")
set(CMAKE_MODULE_PATH ${CMAKE_MODULE_PATH} "${CMAKE_SOURCE_DIR}/tools/cmake/")
# order of linked library is important !
# https://stackoverflow.com/questions/45135/why-does-the-order-in-which-libraries-are-linked-sometimes-cause-errors-in-gcc
set(FAKEVM_LIBS /opt/fake-vm/lib/libfake_vm.a dl rt pthread)
set(FAKEVM_INCLUDE_DIR /opt/fake-vm/include)
set(VSG_INCLUDE_DIR ${CMAKE_CURRENT_SOURCE_DIR}/src/vsg/)

# Search for SimGrid
find_package(SimGrid REQUIRED)

include_directories("${SimGrid_INCLUDE_DIR}" "${FAKEVM_INCLUDE_DIR}" "${VSG_INCLUDE_DIR}"  SYSTEM)

# To build fake-vm/qemu implementation we need to generate the config-host.h header
add_custom_target(config-host.h COMMAND ./configure --target-list=x86_64-softmmu && make -j config-host.h WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}/src/qemu)
# fake-vm client lib (rust implementation)
# installed in /opt/fake-vm
# compiled in RELEASE mode (I've got the power !)
add_custom_target(fake-vm ALL COMMAND make PREFIX=/opt/fake-vm RELEASE=0 install WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}/src/fake-vm)
add_dependencies(fake-vm config-host.h)

# build the qemu base image
add_custom_target(packer COMMAND packer build debian-10.3.0-x86_64.json WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}/packer)

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
configure_file(examples/qemus/boot.py examples/qemus/boot.py COPYONLY)
configure_file(examples/qemus/deployment.xml examples/qemus/deployment.xml COPYONLY)
configure_file(examples/qemus/nova_cluster.xml examples/qemus/nova_cluster.xml COPYONLY)

# Unit tests
add_executable(tests src/tests/tests.cpp src/tests/scenario.cpp)
target_include_directories(tests PUBLIC ${CMAKE_CURRENT_SOURCE_DIR}/src/include)
target_link_libraries(tests PUBLIC ${FAKEVM_LIBS} vsg ${SimGrid_LIBRARY})

add_dependencies(tests fake-vm)

install(TARGETS tansiv DESTINATION bin)
install(TARGETS vsg DESTINATION lib)
install(FILES src/vsg/vsg.h DESTINATION include)
install(CODE "MESSAGE(\"-- Installing libfake_vm\")")
install(CODE "execute_process(COMMAND make PREFIX=/opt/fake-vm RELEASE=0 install WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}/src/fake-vm )")