add_library(tansiv-timer STATIC tansiv-timer.c)
target_compile_options(tansiv-timer PUBLIC -DLOG_USE_COLOR)
target_link_libraries(tansiv-timer PUBLIC m)

target_include_directories(tansiv-timer PUBLIC
          INTERFACE ${CMAKE_CURRENT_SOURCE_DIR}
          )

install(TARGETS tansiv-timer DESTINATION lib)
install(FILES tansiv-timer.h DESTINATION include)