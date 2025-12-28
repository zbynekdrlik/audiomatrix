/**
 * AudioMatrix ASIO Driver COM Loading Test
 *
 * This test loads the driver via COM/DllGetClassObject, exactly as real
 * ASIO hosts like Ableton Live do. This catches issues that in-process
 * testing misses.
 *
 * Test procedure:
 * 1. Load AudioMatrixASIO.dll via LoadLibrary
 * 2. Get DllGetClassObject export
 * 3. Call DllGetClassObject(CLSID_AudioMatrixASIO, IID_IUnknown, &driver)
 * 4. Test the driver interface
 *
 * Exit codes:
 *   0 = All tests passed
 *   1 = DLL load failed
 *   2 = DllGetClassObject failed
 *   3 = Driver interface test failed
 */

#include <windows.h>
#include <initguid.h>
#include <cstdio>
#include <atomic>
#include <chrono>
#include <thread>

// ASIO types
typedef long ASIOError;
typedef long ASIOBool;
typedef long long ASIOSamples;
typedef long long ASIOTimeStamp;
typedef double ASIOSampleRate;

constexpr ASIOBool ASIOTrue = 1;
constexpr ASIOBool ASIOFalse = 0;
constexpr ASIOError ASE_OK = 0;

// Minimal ASIO structures needed for testing
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

// IASIO interface - must match vtable layout exactly
class IASIO : public IUnknown {
public:
    virtual ASIOBool init(void* sysHandle) = 0;
    virtual void getDriverName(char* name) = 0;
    virtual long getDriverVersion() = 0;
    virtual void getErrorMessage(char* string) = 0;
    virtual ASIOError start() = 0;
    virtual ASIOError stop() = 0;
    virtual ASIOError getChannels(long* numInputChannels, long* numOutputChannels) = 0;
    virtual ASIOError getLatencies(long* inputLatency, long* outputLatency) = 0;
    virtual ASIOError getBufferSize(long* minSize, long* maxSize, long* preferredSize, long* granularity) = 0;
    virtual ASIOError canSampleRate(ASIOSampleRate sampleRate) = 0;
    virtual ASIOError getSampleRate(ASIOSampleRate* sampleRate) = 0;
    virtual ASIOError setSampleRate(ASIOSampleRate sampleRate) = 0;
    virtual ASIOError getClockSources(void* clocks, long* numSources) = 0;
    virtual ASIOError setClockSource(long reference) = 0;
    virtual ASIOError getSamplePosition(ASIOSamples* sPos, ASIOTimeStamp* tStamp) = 0;
    virtual ASIOError getChannelInfo(ASIOChannelInfo* info) = 0;
    virtual ASIOError createBuffers(ASIOBufferInfo* bufferInfos, long numChannels, long bufferSize, ASIOCallbacks* callbacks) = 0;
    virtual ASIOError disposeBuffers() = 0;
    virtual ASIOError controlPanel() = 0;
    virtual ASIOError future(long selector, void* opt) = 0;
    virtual ASIOError outputReady() = 0;
};

// CLSID for AudioMatrix Virtual ASIO driver - must match driver's CLSID
// {A1B2C3D4-E5F6-7890-ABCD-EF1234567890}
DEFINE_GUID(CLSID_AudioMatrixASIO,
    0xA1B2C3D4, 0xE5F6, 0x7890,
    0xAB, 0xCD, 0xEF, 0x12, 0x34, 0x56, 0x78, 0x90);

// DllGetClassObject function pointer type
typedef HRESULT (WINAPI *DllGetClassObjectFunc)(REFCLSID, REFIID, LPVOID*);

// Test state
static std::atomic<int> g_bufferSwitchCount{0};
static std::atomic<bool> g_testComplete{false};

// ASIO callbacks
static void bufferSwitch(long doubleBufferIndex, ASIOBool directProcess) {
    (void)doubleBufferIndex;
    (void)directProcess;
    ++g_bufferSwitchCount;
    if (g_bufferSwitchCount >= 10) {
        g_testComplete = true;
    }
}

static void sampleRateDidChange(double sRate) {
    printf("  Sample rate changed to: %.0f\n", sRate);
}

static long asioMessage(long selector, long value, void* message, double* opt) {
    (void)value;
    (void)message;
    (void)opt;
    switch (selector) {
        case 1: return 1;  // kAsioSelectorSupported
        case 4: return 0;  // kAsioSupportsTimeInfo
        default: return 0;
    }
}

static ASIOTime* bufferSwitchTimeInfo(ASIOTime* params, long, ASIOBool) {
    return params;
}

static ASIOCallbacks g_callbacks = {
    bufferSwitch,
    sampleRateDidChange,
    asioMessage,
    bufferSwitchTimeInfo
};

int main(int argc, char* argv[]) {
    printf("=== AudioMatrix ASIO COM Loading Test ===\n\n");

    // Get DLL path from argument or use default
    const char* dllPath = "AudioMatrixASIO.dll";
    if (argc > 1) {
        dllPath = argv[1];
    }

    // Step 1: Load the DLL
    printf("[1] Loading DLL: %s\n", dllPath);
    HMODULE hModule = LoadLibraryA(dllPath);
    if (!hModule) {
        DWORD err = GetLastError();
        printf("FAIL: LoadLibrary failed with error %lu\n", err);
        return 1;
    }
    printf("  OK: DLL loaded at %p\n", (void*)hModule);

    // Step 2: Get DllGetClassObject
    printf("[2] Getting DllGetClassObject...\n");
    DllGetClassObjectFunc pDllGetClassObject =
        (DllGetClassObjectFunc)GetProcAddress(hModule, "DllGetClassObject");
    if (!pDllGetClassObject) {
        printf("FAIL: GetProcAddress(DllGetClassObject) failed\n");
        FreeLibrary(hModule);
        return 1;
    }
    printf("  OK: DllGetClassObject at %p\n", (void*)pDllGetClassObject);

    // Step 3: Call DllGetClassObject to get driver instance
    printf("[3] Calling DllGetClassObject...\n");
    IASIO* driver = nullptr;
    HRESULT hr = pDllGetClassObject(CLSID_AudioMatrixASIO, IID_IUnknown, (void**)&driver);
    if (FAILED(hr)) {
        printf("FAIL: DllGetClassObject returned 0x%08lX\n", hr);
        FreeLibrary(hModule);
        return 2;
    }
    if (!driver) {
        printf("FAIL: DllGetClassObject returned NULL driver\n");
        FreeLibrary(hModule);
        return 2;
    }
    printf("  OK: Got IASIO interface at %p\n", (void*)driver);

    // Step 4: Initialize driver
    printf("[4] Initializing driver...\n");
    ASIOBool initResult = driver->init(nullptr);
    if (initResult != ASIOTrue) {
        char errorMsg[128] = {0};
        driver->getErrorMessage(errorMsg);
        printf("FAIL: init() returned false - %s\n", errorMsg);
        driver->Release();
        FreeLibrary(hModule);
        return 3;
    }
    printf("  OK: Driver initialized\n");

    // Get driver info
    char driverName[32] = {0};
    driver->getDriverName(driverName);
    long driverVersion = driver->getDriverVersion();
    printf("  Driver: %s v%ld\n", driverName, driverVersion);

    // Step 5: Query capabilities
    printf("[5] Querying capabilities...\n");
    long numInputChannels = 0, numOutputChannels = 0;
    ASIOError err = driver->getChannels(&numInputChannels, &numOutputChannels);
    if (err != ASE_OK) {
        printf("FAIL: getChannels() returned %ld\n", err);
        driver->Release();
        FreeLibrary(hModule);
        return 3;
    }
    printf("  Channels: %ld in, %ld out\n", numInputChannels, numOutputChannels);

    long minSize = 0, maxSize = 0, preferredSize = 0, granularity = 0;
    err = driver->getBufferSize(&minSize, &maxSize, &preferredSize, &granularity);
    if (err != ASE_OK) {
        printf("FAIL: getBufferSize() returned %ld\n", err);
        driver->Release();
        FreeLibrary(hModule);
        return 3;
    }
    printf("  Buffer size: min=%ld, max=%ld, preferred=%ld\n", minSize, maxSize, preferredSize);

    ASIOSampleRate sampleRate = 0;
    err = driver->getSampleRate(&sampleRate);
    if (err != ASE_OK) {
        printf("FAIL: getSampleRate() returned %ld\n", err);
        driver->Release();
        FreeLibrary(hModule);
        return 3;
    }
    printf("  Sample rate: %.0f Hz\n", sampleRate);
    printf("  OK: All capability queries passed\n");

    // Step 6: Create buffers
    printf("[6] Creating buffers...\n");
    ASIOBufferInfo bufferInfos[4];
    for (int i = 0; i < 2; ++i) {
        bufferInfos[i].isInput = ASIOTrue;
        bufferInfos[i].channelNum = i;
        bufferInfos[i].buffers[0] = nullptr;
        bufferInfos[i].buffers[1] = nullptr;
    }
    for (int i = 0; i < 2; ++i) {
        bufferInfos[2 + i].isInput = ASIOFalse;
        bufferInfos[2 + i].channelNum = i;
        bufferInfos[2 + i].buffers[0] = nullptr;
        bufferInfos[2 + i].buffers[1] = nullptr;
    }

    err = driver->createBuffers(bufferInfos, 4, preferredSize, &g_callbacks);
    if (err != ASE_OK) {
        printf("FAIL: createBuffers() returned %ld\n", err);
        driver->Release();
        FreeLibrary(hModule);
        return 3;
    }

    bool buffersValid = true;
    for (int i = 0; i < 4; ++i) {
        if (!bufferInfos[i].buffers[0] || !bufferInfos[i].buffers[1]) {
            buffersValid = false;
            break;
        }
    }
    if (!buffersValid) {
        printf("FAIL: Buffer pointers not set\n");
        driver->disposeBuffers();
        driver->Release();
        FreeLibrary(hModule);
        return 3;
    }
    printf("  OK: Buffers created\n");

    // Step 7: Start audio and test callbacks
    printf("[7] Starting audio...\n");
    err = driver->start();
    if (err != ASE_OK) {
        printf("FAIL: start() returned %ld\n", err);
        driver->disposeBuffers();
        driver->Release();
        FreeLibrary(hModule);
        return 3;
    }
    printf("  OK: Audio started\n");

    printf("[8] Testing audio callbacks...\n");
    auto startTime = std::chrono::steady_clock::now();
    while (!g_testComplete) {
        std::this_thread::sleep_for(std::chrono::milliseconds(10));
        if (std::chrono::steady_clock::now() - startTime > std::chrono::seconds(3)) {
            break;
        }
    }

    int callbackCount = g_bufferSwitchCount.load();
    printf("  Buffer switch callbacks: %d\n", callbackCount);
    if (callbackCount < 10) {
        printf("FAIL: Expected >= 10 callbacks\n");
        driver->stop();
        driver->disposeBuffers();
        driver->Release();
        FreeLibrary(hModule);
        return 3;
    }
    printf("  OK: Audio callbacks working\n");

    // Cleanup
    printf("[9] Cleanup...\n");
    driver->stop();
    driver->disposeBuffers();
    driver->Release();
    printf("  OK: Driver released\n");

    FreeLibrary(hModule);
    printf("  OK: DLL unloaded\n");

    printf("\n=== ALL COM LOADING TESTS PASSED ===\n");
    return 0;
}
