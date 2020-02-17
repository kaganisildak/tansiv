add_library(vsg STATIC vsg.c log.c)
target_compile_options(vsg PUBLIC -DLOG_USE_COLOR)

target_include_directories(vsg PUBLIC
          INTERFACE ${CMAKE_CURRENT_SOURCE_DIR}
          )

install(TARGETS vsg DESTINATION lib)
install(FILES vsg.h DESTINATION include)