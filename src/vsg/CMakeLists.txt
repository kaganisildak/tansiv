add_library(vsg vsg.c)

# state that anybody linking to us needs to include the current source dir
# to find MathFunctions.h, while we don't.
target_include_directories(vsg
          INTERFACE ${CMAKE_CURRENT_SOURCE_DIR}
          )

install(TARGETS vsg DESTINATION lib)
install(FILES vsg.h DESTINATION include)