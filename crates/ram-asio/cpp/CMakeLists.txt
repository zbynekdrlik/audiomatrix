cmake_minimum_required(VERSION 3.20)
project(AudioMatrixASIO VERSION 0.1.0 LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# Windows-only project
if(NOT WIN32)
    message(FATAL_ERROR "AudioMatrix Virtual ASIO driver is Windows-only")
endif()

# Note: We define all ASIO types/structs in virtual_asio.h, so no SDK dependency.
# This avoids compatibility issues with the SDK's ANSI-mode code.

# Driver sources only
set(DRIVER_SOURCES
    virtual_asio.cpp
    shared_memory.cpp
    registry.cpp
    dllmain.cpp
)

# Create shared library (DLL)
add_library(AudioMatrixASIO SHARED
    ${DRIVER_SOURCES}
    AudioMatrixASIO.def
)

# Include directories
target_include_directories(AudioMatrixASIO PRIVATE
    ${CMAKE_SOURCE_DIR}
)

# Link libraries
target_link_libraries(AudioMatrixASIO PRIVATE
    ole32
    oleaut32
    uuid
    advapi32
    winmm
)

# Compiler flags
# Note: Don't use WIN32_LEAN_AND_MEAN as it excludes COM definitions
# Note: UNICODE/_UNICODE for our code only (registry.cpp uses wide APIs)
target_compile_definitions(AudioMatrixASIO PRIVATE
    UNICODE
    _UNICODE
    NOMINMAX
)

if(MSVC)
    target_compile_options(AudioMatrixASIO PRIVATE
        /W4     # Warning level 4
        /WX     # Treat warnings as errors
        /MP     # Multi-processor compilation
    )
endif()

# Output name
set_target_properties(AudioMatrixASIO PROPERTIES
    OUTPUT_NAME "AudioMatrixASIO"
    PREFIX ""
)

# Generate .def file for COM exports
file(WRITE ${CMAKE_BINARY_DIR}/AudioMatrixASIO.def
"LIBRARY AudioMatrixASIO
EXPORTS
    DllGetClassObject PRIVATE
    DllCanUnloadNow PRIVATE
    DllRegisterServer PRIVATE
    DllUnregisterServer PRIVATE
")

# =============================================================================
# ASIO Driver Test Executable
# =============================================================================
add_executable(asio_test
    asio_test.cpp
    virtual_asio.cpp
    shared_memory.cpp
)

target_include_directories(asio_test PRIVATE
    ${CMAKE_SOURCE_DIR}
)

target_link_libraries(asio_test PRIVATE
    ole32
    oleaut32
    uuid
    advapi32
    winmm
)

target_compile_definitions(asio_test PRIVATE
    UNICODE
    _UNICODE
    NOMINMAX
)

if(MSVC)
    target_compile_options(asio_test PRIVATE
        /W4
        /WX
        /MP
    )
endif()

# =============================================================================
# COM Loading Test (tests actual ASIO host loading path)
# =============================================================================
add_executable(asio_com_test
    asio_com_test.cpp
)

target_link_libraries(asio_com_test PRIVATE
    ole32
    uuid
)

target_compile_definitions(asio_com_test PRIVATE
    UNICODE
    _UNICODE
    NOMINMAX
)

if(MSVC)
    target_compile_options(asio_com_test PRIVATE
        /W4
        /WX
        /MP
    )
endif()

# Enable CTest
enable_testing()

# In-process test (direct factory call)
add_test(NAME asio_driver_test COMMAND asio_test)

# COM loading test (mimics how Ableton loads the driver)
add_test(NAME asio_com_loading_test
    COMMAND asio_com_test "$<TARGET_FILE:AudioMatrixASIO>"
)

# =============================================================================
# Install targets
# =============================================================================
install(TARGETS AudioMatrixASIO
    RUNTIME DESTINATION bin
    LIBRARY DESTINATION lib
)

install(TARGETS asio_test
    RUNTIME DESTINATION bin
)

# Installation script
install(CODE "
    message(STATUS \"To register the ASIO driver, run as Administrator:\")
    message(STATUS \"  regsvr32 \\\"${CMAKE_INSTALL_PREFIX}/bin/AudioMatrixASIO.dll\\\"\")
    message(STATUS \"\")
    message(STATUS \"To unregister:\")
    message(STATUS \"  regsvr32 /u \\\"${CMAKE_INSTALL_PREFIX}/bin/AudioMatrixASIO.dll\\\"\")
")

# Custom target for registration
add_custom_target(register
    COMMAND regsvr32 "$<TARGET_FILE:AudioMatrixASIO>"
    DEPENDS AudioMatrixASIO
    COMMENT "Registering AudioMatrix ASIO driver..."
    VERBATIM
)

add_custom_target(unregister
    COMMAND regsvr32 /u "$<TARGET_FILE:AudioMatrixASIO>"
    DEPENDS AudioMatrixASIO
    COMMENT "Unregistering AudioMatrix ASIO driver..."
    VERBATIM
)
