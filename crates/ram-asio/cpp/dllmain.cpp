/**
 * AudioMatrix Virtual ASIO Driver - DLL Entry Point
 *
 * Standard Windows DLL entry point for the ASIO COM driver.
 */

#include "virtual_asio.h"
#include <objbase.h>

HINSTANCE g_hInstance = nullptr;
static long g_serverLockCount = 0;

BOOL WINAPI DllMain(HINSTANCE hInstDLL, DWORD fdwReason, LPVOID /*lpvReserved*/) {
    switch (fdwReason) {
        case DLL_PROCESS_ATTACH:
            g_hInstance = hInstDLL;
            DisableThreadLibraryCalls(hInstDLL);
            break;

        case DLL_PROCESS_DETACH:
            audiomatrix::VirtualAsioDriverFactory::releaseInstance();
            break;

        case DLL_THREAD_ATTACH:
        case DLL_THREAD_DETACH:
            break;
    }

    return TRUE;
}

// COM exports - use STDAPI to match Windows SDK linkage

STDAPI DllGetClassObject(REFCLSID /*rclsid*/, REFIID /*riid*/, LPVOID* ppv) {
    // For ASIO, we return the driver instance directly
    // ASIO doesn't use COM class factories in the traditional sense
    *ppv = audiomatrix::VirtualAsioDriverFactory::getInstance();
    return *ppv ? S_OK : E_OUTOFMEMORY;
}

STDAPI DllCanUnloadNow() {
    // Can unload when no server locks and no instances
    return (g_serverLockCount == 0) ? S_OK : S_FALSE;
}

// DllRegisterServer and DllUnregisterServer are in registry.cpp
