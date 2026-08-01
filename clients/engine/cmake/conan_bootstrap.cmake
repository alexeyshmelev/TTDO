if(NOT EXISTS "${CMAKE_CURRENT_LIST_DIR}/conan_provider.cmake")
    message(FATAL_ERROR "The vendored Conan provider is missing from the source tree")
endif()

include("${CMAKE_CURRENT_LIST_DIR}/conan_provider.cmake")
