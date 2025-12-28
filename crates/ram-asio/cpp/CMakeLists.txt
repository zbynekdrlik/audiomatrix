cmake_minimum_required(VERSION 3.20)
project(AudioMatrixASIO VERSION 0.1.0 LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# Windows-only project
if(NOT WIN32)
    message(FATAL_ERROR "AudioMatrix Virtual ASIO driver is Windows-only")
endif()

# Find ASIO SDK
set(ASIO_SDK_DIR "${CMAKE_SOURCE_DIR}/external/asio_sdk" CACHE PATH "Path to ASIO SDK")
if(NOT EXISTS "${ASIO_SDK_DIR}/common/asio.h")
    message(FATAL_ERROR "ASIO SDK not found at ${ASIO_SDK_DIR}. "
            "Clone it with: git clone https://github.com/audiosdk/asio.git external/asio_sdk")
endif()

# ASIO SDK sources (required by ASIO specification)
set(ASIO_SDK_SOURCES
    ${ASIO_SDK_DIR}/common/asio.cpp
    ${ASIO_SDK_DIR}/host/asiodrivers.cpp
    ${ASIO_SDK_DIR}/host/pc/asiolist.cpp
)

# Driver sources
set(DRIVER_SOURCES
    virtual_asio.cpp
    shared_memory.cpp
    registry.cpp
    dllmain.cpp
)

# Create shared library (DLL)
add_library(AudioMatrixASIO SHARED
    ${DRIVER_SOURCES}
    ${ASIO_SDK_SOURCES}
    AudioMatrixASIO.def
)

# Include directories
target_include_directories(AudioMatrixASIO PRIVATE
    ${CMAKE_SOURCE_DIR}
    ${ASIO_SDK_DIR}/common
    ${ASIO_SDK_DIR}/host
    ${ASIO_SDK_DIR}/host/pc
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
target_compile_definitions(AudioMatrixASIO PRIVATE
    UNICODE
    _UNICODE
    WIN32_LEAN_AND_MEAN
    NOMINMAX
    # ASIO SDK uses 'interface' keyword (Microsoft extension)
    "interface=struct"
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

# Install target
install(TARGETS AudioMatrixASIO
    RUNTIME DESTINATION bin
    LIBRARY DESTINATION lib
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
