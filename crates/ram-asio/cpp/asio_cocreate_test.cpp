/**
 * AudioMatrix ASIO CoCreateInstance Test
 *
 * Tests loading the driver via CoCreateInstance(), which is the
 * standard COM API. Some ASIO hosts may use this instead of
 * direct LoadLibrary/DllGetClassObject calls.
 */

#include <windows.h>
#include <initguid.h>
#include <objbase.h>
#include <cstdio>

// CLSID for AudioMatrix Virtual ASIO driver
// {A1B2C3D4-E5F6-7890-ABCD-EF1234567890}
DEFINE_GUID(CLSID_AudioMatrixASIO,
    0xA1B2C3D4, 0xE5F6, 0x7890,
    0xAB, 0xCD, 0xEF, 0x12, 0x34, 0x56, 0x78, 0x90);

int main() {
    printf("=== AudioMatrix ASIO CoCreateInstance Test ===\n\n");

    // Initialize COM
    printf("[1] Initializing COM...\n");
    HRESULT hr = CoInitializeEx(nullptr, COINIT_APARTMENTTHREADED);
    if (FAILED(hr)) {
        printf("FAIL: CoInitializeEx failed: 0x%08lX\n", hr);
        return 1;
    }
    printf("  OK: COM initialized\n");

    // Try CoCreateInstance
    printf("[2] Calling CoCreateInstance...\n");
    IUnknown* pUnk = nullptr;
    hr = CoCreateInstance(
        CLSID_AudioMatrixASIO,
        nullptr,
        CLSCTX_INPROC_SERVER,
        IID_IUnknown,
        (void**)&pUnk
    );

    if (FAILED(hr)) {
        printf("FAIL: CoCreateInstance failed: 0x%08lX\n", hr);
        if (hr == REGDB_E_CLASSNOTREG) {
            printf("  Error: Class not registered (REGDB_E_CLASSNOTREG)\n");
            printf("  Check: reg query \"HKLM\\SOFTWARE\\Classes\\CLSID\\{A1B2C3D4-E5F6-7890-ABCD-EF1234567890}\"\n");
        } else if (hr == CLASS_E_NOAGGREGATION) {
            printf("  Error: Class does not support aggregation\n");
        } else if (hr == E_NOINTERFACE) {
            printf("  Error: Interface not supported (E_NOINTERFACE)\n");
        }
        CoUninitialize();
        return 2;
    }

    if (!pUnk) {
        printf("FAIL: CoCreateInstance returned NULL\n");
        CoUninitialize();
        return 2;
    }

    printf("  OK: Got IUnknown at %p\n", (void*)pUnk);

    // Release
    printf("[3] Releasing...\n");
    pUnk->Release();
    printf("  OK: Released\n");

    CoUninitialize();
    printf("  OK: COM uninitialized\n");

    printf("\n=== COCREATEINSTANCE TEST PASSED ===\n");
    return 0;
}
