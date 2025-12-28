/**
 * AudioMatrix Virtual ASIO Driver - DLL Entry Point
 *
 * Standard Windows DLL entry point for the ASIO COM driver.
 */

#include "virtual_asio.h"

HINSTANCE g_hInstance = nullptr;

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
