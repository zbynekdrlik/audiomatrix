/**
 * AudioMatrix ASIO Vtable Validation Test
 *
 * This test validates the vtable layout by calling each method
 * and ensuring no crashes occur. This catches vtable misalignment
 * issues that cause real ASIO hosts to crash.
 */

#include <windows.h>
#include <initguid.h>
#include <cstdio>
#include <cstring>

// ASIO types
typedef long ASIOError;
typedef long ASIOBool;
typedef long long ASIOSamples;
typedef long long ASIOTimeStamp;
typedef double ASIOSampleRate;

constexpr ASIOBool ASIOTrue = 1;
constexpr ASIOBool ASIOFalse = 0;
constexpr ASIOError ASE_OK = 0;

struct ASIOClockSource {
    long index;
    long associatedChannel;
    long associatedGroup;
    ASIOBool isCurrentSource;
    char name[32];
};

struct ASIOChannelInfo {
    long channel;
    ASIOBool isInput;
    ASIOBool isActive;
    long channelGroup;
    long type;
    char name[32];
};

struct ASIOBufferInfo {
    ASIOBool isInput;
    long channelNum;
    void* buffers[2];
};

struct ASIOTime {
    long reserved[4];
    char data[128];
};

struct ASIOCallbacks {
    void (*bufferSwitch)(long doubleBufferIndex, ASIOBool directProcess);
    void (*sampleRateDidChange)(double sRate);
    long (*asioMessage)(long selector, long value, void* message, double* opt);
    ASIOTime* (*bufferSwitchTimeInfo)(ASIOTime* params, long doubleBufferIndex, ASIOBool directProcess);
};

// IASIO vtable structure - this is what the vtable actually looks like in memory
// Each entry is a function pointer
struct IASIOVtable {
    // IUnknown (slots 0-2)
    HRESULT (__stdcall *QueryInterface)(void* pThis, REFIID riid, void** ppv);
    ULONG (__stdcall *AddRef)(void* pThis);
    ULONG (__stdcall *Release)(void* pThis);

    // IASIO (slots 3-23) - NOTE: these use __thiscall, not __stdcall!
    ASIOBool (__thiscall *init)(void* pThis, void* sysHandle);
    void (__thiscall *getDriverName)(void* pThis, char* name);
    long (__thiscall *getDriverVersion)(void* pThis);
    void (__thiscall *getErrorMessage)(void* pThis, char* string);
    ASIOError (__thiscall *start)(void* pThis);
    ASIOError (__thiscall *stop)(void* pThis);
    ASIOError (__thiscall *getChannels)(void* pThis, long* numInputChannels, long* numOutputChannels);
    ASIOError (__thiscall *getLatencies)(void* pThis, long* inputLatency, long* outputLatency);
    ASIOError (__thiscall *getBufferSize)(void* pThis, long* minSize, long* maxSize, long* preferredSize, long* granularity);
    ASIOError (__thiscall *canSampleRate)(void* pThis, ASIOSampleRate sampleRate);
    ASIOError (__thiscall *getSampleRate)(void* pThis, ASIOSampleRate* sampleRate);
    ASIOError (__thiscall *setSampleRate)(void* pThis, ASIOSampleRate sampleRate);
    ASIOError (__thiscall *getClockSources)(void* pThis, ASIOClockSource* clocks, long* numSources);
    ASIOError (__thiscall *setClockSource)(void* pThis, long reference);
    ASIOError (__thiscall *getSamplePosition)(void* pThis, ASIOSamples* sPos, ASIOTimeStamp* tStamp);
    ASIOError (__thiscall *getChannelInfo)(void* pThis, ASIOChannelInfo* info);
    ASIOError (__thiscall *createBuffers)(void* pThis, ASIOBufferInfo* bufferInfos, long numChannels, long bufferSize, ASIOCallbacks* callbacks);
    ASIOError (__thiscall *disposeBuffers)(void* pThis);
    ASIOError (__thiscall *controlPanel)(void* pThis);
    ASIOError (__thiscall *future)(void* pThis, long selector, void* opt);
    ASIOError (__thiscall *outputReady)(void* pThis);
};

// COM object structure
struct COMObject {
    IASIOVtable* vtable;
    // ... instance data follows
};

// CLSID
DEFINE_GUID(CLSID_AudioMatrixASIO,
    0xA1B2C3D4, 0xE5F6, 0x7890,
    0xAB, 0xCD, 0xEF, 0x12, 0x34, 0x56, 0x78, 0x90);

typedef HRESULT (WINAPI *DllGetClassObjectFunc)(REFCLSID, REFIID, LPVOID*);

int main(int argc, char* argv[]) {
    printf("=== AudioMatrix ASIO Vtable Validation Test ===\n\n");

    const char* dllPath = "AudioMatrixASIO.dll";
    if (argc > 1) dllPath = argv[1];

    // Load DLL
    printf("[1] Loading DLL: %s\n", dllPath);
    HMODULE hModule = LoadLibraryA(dllPath);
    if (!hModule) {
        printf("FAIL: LoadLibrary failed: %lu\n", GetLastError());
        return 1;
    }

    // Get DllGetClassObject
    DllGetClassObjectFunc pDllGetClassObject =
        (DllGetClassObjectFunc)GetProcAddress(hModule, "DllGetClassObject");
    if (!pDllGetClassObject) {
        printf("FAIL: GetProcAddress failed\n");
        FreeLibrary(hModule);
        return 1;
    }

    // Get driver instance
    printf("[2] Getting driver instance...\n");
    void* pDriver = nullptr;
    HRESULT hr = pDllGetClassObject(CLSID_AudioMatrixASIO, IID_IUnknown, &pDriver);
    if (FAILED(hr) || !pDriver) {
        printf("FAIL: DllGetClassObject failed: 0x%08lX\n", hr);
        FreeLibrary(hModule);
        return 1;
    }
    printf("  Driver at: %p\n", pDriver);

    // Get vtable pointer
    COMObject* pObj = (COMObject*)pDriver;
    IASIOVtable* vtable = pObj->vtable;
    printf("  Vtable at: %p\n", (void*)vtable);

    // Print vtable entries for debugging
    printf("\n[3] Vtable entries:\n");
    void** vt = (void**)vtable;
    for (int i = 0; i < 24; i++) {
        printf("  [%2d] %p\n", i, vt[i]);
    }

    // Test each method via vtable
    printf("\n[4] Testing vtable methods...\n");

    // Test slot 3: init
    printf("  Testing init (slot 3)...\n");
    ASIOBool initResult = vtable->init(pDriver, nullptr);
    printf("    init returned: %ld\n", initResult);

    // Test slot 4: getDriverName
    printf("  Testing getDriverName (slot 4)...\n");
    char driverName[32] = {0};
    vtable->getDriverName(pDriver, driverName);
    printf("    Driver name: %s\n", driverName);

    // Test slot 5: getDriverVersion
    printf("  Testing getDriverVersion (slot 5)...\n");
    long version = vtable->getDriverVersion(pDriver);
    printf("    Version: %ld\n", version);

    // Test slot 6: getErrorMessage
    printf("  Testing getErrorMessage (slot 6)...\n");
    char errorMsg[128] = {0};
    vtable->getErrorMessage(pDriver, errorMsg);
    printf("    Error message: %s\n", errorMsg[0] ? errorMsg : "(empty)");

    // Test slot 10: getChannels
    printf("  Testing getChannels (slot 10)...\n");
    long numIn = 0, numOut = 0;
    ASIOError err = vtable->getChannels(pDriver, &numIn, &numOut);
    printf("    Channels: %ld in, %ld out (err=%ld)\n", numIn, numOut, err);

    // Test slot 12: getBufferSize
    printf("  Testing getBufferSize (slot 12)...\n");
    long minSize = 0, maxSize = 0, prefSize = 0, gran = 0;
    err = vtable->getBufferSize(pDriver, &minSize, &maxSize, &prefSize, &gran);
    printf("    Buffer size: min=%ld, max=%ld, pref=%ld (err=%ld)\n", minSize, maxSize, prefSize, err);

    // Test slot 14: getSampleRate
    printf("  Testing getSampleRate (slot 14)...\n");
    ASIOSampleRate sampleRate = 0;
    err = vtable->getSampleRate(pDriver, &sampleRate);
    printf("    Sample rate: %.0f (err=%ld)\n", sampleRate, err);

    // Test slot 19: getChannelInfo
    printf("  Testing getChannelInfo (slot 19)...\n");
    ASIOChannelInfo info = {0};
    info.channel = 0;
    info.isInput = ASIOFalse;
    err = vtable->getChannelInfo(pDriver, &info);
    printf("    Channel 0 out: type=%ld, name=%s (err=%ld)\n", info.type, info.name, err);

    // Cleanup via vtable
    printf("\n[5] Cleanup via Release (slot 2)...\n");
    ULONG refCount = vtable->Release(pDriver);
    printf("  Release returned: %lu\n", refCount);

    FreeLibrary(hModule);
    printf("\n=== VTABLE TEST COMPLETE ===\n");
    return 0;
}
