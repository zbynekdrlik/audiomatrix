/**
 * AudioMatrix Virtual ASIO Driver
 *
 * Implements the ASIO interface to allow DAWs to route audio through AudioMatrix.
 * Communicates with the AudioMatrix service via shared memory for low-latency
 * audio transfer.
 *
 * License: GPLv3 (uses Steinberg ASIO SDK)
 */

#pragma once

#include <windows.h>
#include <atomic>
#include <string>
#include <vector>
#include <thread>

// ASIO types (from ASIO SDK specification)
typedef long ASIOError;
typedef long ASIOBool;
typedef long long ASIOSamples;
typedef long long ASIOTimeStamp;

// ASIO boolean values
constexpr ASIOBool ASIOTrue = 1;
constexpr ASIOBool ASIOFalse = 0;

// Forward declare ASIO types
struct ASIODriverInfo;
struct ASIOClockSource;
struct ASIOChannelInfo;
struct ASIOBufferInfo;
struct ASIOTime;

// ASIOCallbacks - must be fully defined for member access
struct ASIOCallbacks {
    void (*bufferSwitch)(long doubleBufferIndex, ASIOBool directProcess);
    void (*sampleRateDidChange)(double sRate);
    long (*asioMessage)(long selector, long value, void* message, double* opt);
    ASIOTime* (*bufferSwitchTimeInfo)(ASIOTime* params, long doubleBufferIndex, ASIOBool directProcess);
};

// ASIO error codes
constexpr ASIOError ASE_OK = 0;
constexpr ASIOError ASE_SUCCESS = 0x3f4847a0;
constexpr ASIOError ASE_NotPresent = -1000;
constexpr ASIOError ASE_HWMalfunction = -999;
constexpr ASIOError ASE_InvalidParameter = -998;
constexpr ASIOError ASE_InvalidMode = -997;
constexpr ASIOError ASE_SPNotAdvancing = -996;
constexpr ASIOError ASE_NoClock = -995;
constexpr ASIOError ASE_NoMemory = -994;

// ASIO sample types
constexpr long ASIOSTInt16MSB = 0;
constexpr long ASIOSTInt24MSB = 1;
constexpr long ASIOSTInt32MSB = 2;
constexpr long ASIOSTFloat32MSB = 3;
constexpr long ASIOSTFloat64MSB = 4;
constexpr long ASIOSTInt32MSB16 = 8;
constexpr long ASIOSTInt32MSB18 = 9;
constexpr long ASIOSTInt32MSB20 = 10;
constexpr long ASIOSTInt32MSB24 = 11;
constexpr long ASIOSTInt16LSB = 16;
constexpr long ASIOSTInt24LSB = 17;
constexpr long ASIOSTInt32LSB = 18;
constexpr long ASIOSTFloat32LSB = 19;
constexpr long ASIOSTFloat64LSB = 20;
constexpr long ASIOSTInt32LSB16 = 24;
constexpr long ASIOSTInt32LSB18 = 25;
constexpr long ASIOSTInt32LSB20 = 26;
constexpr long ASIOSTInt32LSB24 = 27;

namespace audiomatrix {

/**
 * Shared memory header for IPC with AudioMatrix service.
 * Must match the Rust side definition in ram-asio/src/virtual_device.rs
 */
#pragma pack(push, 1)
struct SharedMemoryHeader {
    uint64_t magic;          // Magic number for validation: 0x41554449_4F4D5458 ("AUDIOMTX")
    uint32_t version;        // Protocol version
    uint32_t channels;       // Number of channels
    uint32_t sample_rate;    // Sample rate in Hz
    uint32_t buffer_size;    // Buffer size in samples per channel
    uint64_t read_pos;       // Ring buffer read position (atomic)
    uint64_t write_pos;      // Ring buffer write position (atomic)
    uint64_t flags;          // Status flags
    uint8_t reserved[16];    // Reserved for future use
};
#pragma pack(pop)

static_assert(sizeof(SharedMemoryHeader) == 64, "SharedMemoryHeader must be 64 bytes");

constexpr uint64_t SHARED_MEMORY_MAGIC = 0x41554449'4F4D5458ULL; // "AUDIOMTX"
constexpr uint32_t SHARED_MEMORY_VERSION = 1;

// Flags
constexpr uint64_t FLAG_SERVICE_CONNECTED = 1 << 0;
constexpr uint64_t FLAG_DRIVER_RUNNING = 1 << 1;
constexpr uint64_t FLAG_BUFFER_SWITCH_REQ = 1 << 2;

/**
 * Virtual ASIO Driver implementation.
 *
 * This class implements the ASIO interface and manages communication
 * with the AudioMatrix service via shared memory.
 */
class VirtualAsioDriver {
public:
    VirtualAsioDriver();
    ~VirtualAsioDriver();

    // Prevent copying
    VirtualAsioDriver(const VirtualAsioDriver&) = delete;
    VirtualAsioDriver& operator=(const VirtualAsioDriver&) = delete;

    // ASIO Interface Methods
    ASIOError init(void* sysRef);
    void getDriverName(char* name);
    long getDriverVersion();
    void getErrorMessage(char* message);

    ASIOError start();
    ASIOError stop();

    ASIOError getChannels(long* numInputChannels, long* numOutputChannels);
    ASIOError getLatencies(long* inputLatency, long* outputLatency);
    ASIOError getBufferSize(long* minSize, long* maxSize, long* preferredSize, long* granularity);
    ASIOError canSampleRate(double sampleRate);
    ASIOError getSampleRate(double* sampleRate);
    ASIOError setSampleRate(double sampleRate);

    ASIOError getClockSources(ASIOClockSource* clocks, long* numSources);
    ASIOError setClockSource(long reference);

    ASIOError getSamplePosition(ASIOSamples* sPos, ASIOTimeStamp* tStamp);
    ASIOError getChannelInfo(ASIOChannelInfo* info);

    ASIOError createBuffers(ASIOBufferInfo* bufferInfos, long numChannels,
                            long bufferSize, ASIOCallbacks* callbacks);
    ASIOError disposeBuffers();

    ASIOError controlPanel();
    ASIOError future(long selector, void* opt);
    ASIOError outputReady();

private:
    // Shared memory management
    bool openSharedMemory(const std::wstring& name);
    void closeSharedMemory();

    // Audio processing thread
    void audioThreadProc();
    void processBuffers();

    // State
    bool m_initialized = false;
    bool m_running = false;
    std::atomic<bool> m_stopRequested{false};

    // Configuration
    long m_numInputChannels = 2;
    long m_numOutputChannels = 2;
    double m_sampleRate = 48000.0;
    long m_bufferSize = 256;

    // Shared memory
    HANDLE m_sharedMemHandle = nullptr;
    void* m_sharedMemPtr = nullptr;
    size_t m_sharedMemSize = 0;
    SharedMemoryHeader* m_header = nullptr;
    float* m_inputBuffer = nullptr;
    float* m_outputBuffer = nullptr;

    // ASIO callbacks
    ASIOCallbacks* m_callbacks = nullptr;

    // Buffer management
    std::vector<ASIOBufferInfo> m_bufferInfos;
    std::vector<std::vector<float>> m_inputBuffers[2];  // Double buffer
    std::vector<std::vector<float>> m_outputBuffers[2]; // Double buffer
    long m_currentBuffer = 0;

    // Audio thread
    std::thread m_audioThread;
    HANDLE m_bufferEvent = nullptr;

    // Sample position tracking
    ASIOSamples m_samplePosition = 0;

    // Error message
    char m_errorMessage[128] = {0};

    // Device name
    std::string m_deviceName = "AudioMatrix Virtual";
};

/**
 * COM Class Factory for the ASIO driver.
 * Windows COM infrastructure uses this to create driver instances.
 */
class VirtualAsioDriverFactory {
public:
    static VirtualAsioDriver* getInstance();
    static void releaseInstance();

private:
    static VirtualAsioDriver* s_instance;
    static long s_refCount;
};

} // namespace audiomatrix

// COM DLL exports
extern "C" {
    __declspec(dllexport) HRESULT DllGetClassObject(REFCLSID rclsid, REFIID riid, LPVOID* ppv);
    __declspec(dllexport) HRESULT DllCanUnloadNow();
    __declspec(dllexport) HRESULT DllRegisterServer();
    __declspec(dllexport) HRESULT DllUnregisterServer();
}
