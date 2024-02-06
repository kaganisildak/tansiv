add_library(tansiv-timer-xen STATIC tansiv-timer-xen.c)
target_compile_options(tansiv-timer-xen PUBLIC -DLOG_USE_COLOR)
target_link_libraries(tansiv-timer-xen PUBLIC m)

target_include_directories(tansiv-timer-xen PUBLIC
          INTERFACE ${CMAKE_CURRENT_SOURCE_DIR}
          )

install(TARGETS tansiv-timer-xen DESTINATION lib)
install(FILES tansiv-timer-xen.h DESTINATION include)