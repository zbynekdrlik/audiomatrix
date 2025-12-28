/**
 * AudioMatrix Virtual ASIO Driver
 *
 * Implements the ASIO interface to allow DAWs to route audio through AudioMatrix.
 * Communicates with the AudioMatrix service via shared memory for low-latency
 * audio transfer.
 *
 * License: GPLv3 (uses Steinberg ASIO SDK interface specification)
 */

#pragma once

#include <windows.h>
#include <unknwn.h>
#include <atomic>
#include <string>
#include <vector>
#include <thread>

// ASIO types (from ASIO SDK specification)
typedef long ASIOError;
typedef long ASIOBool;
typedef long long ASIOSamples;
typedef long long ASIOTimeStamp;
typedef double ASIOSampleRate;

// ASIO boolean values
constexpr ASIOBool ASIOTrue = 1;
constexpr ASIOBool ASIOFalse = 0;

// ASIO struct definitions (from ASIO specification)
struct ASIODriverInfo {
    long asioVersion;       // Currently 2
    long driverVersion;     // Driver version
    char name[32];          // Driver name
    char errorMessage[124]; // Error message
    void* sysRef;           // System reference (HWND on Windows)
};

struct ASIOClockSource {
    long index;             // Clock source index
    long associatedChannel; // Channel if sample rate derived from input
    long associatedGroup;   // Group index
    ASIOBool isCurrentSource;
    char name[32];          // Clock source name
};

struct ASIOChannelInfo {
    long channel;           // Channel index
    ASIOBool isInput;       // True for input, false for output
    ASIOBool isActive;      // True if channel is active
    long channelGroup;      // Optional group index
    long type;              // Sample type (ASIOSTxxxx)
    char name[32];          // Channel name
};

struct ASIOBufferInfo {
    ASIOBool isInput;       // True for input, false for output
    long channelNum;        // Channel index
    void* buffers[2];       // Double buffer pointers
};

struct ASIOTimeCode {
    double speed;           // Speed relation (default = 1.0)
    ASIOSamples timeCodeSamples;
    unsigned long flags;    // Time code flags
    char future[64];
};

struct AsioTimeInfo {
    double speed;           // Speed relation
    ASIOTimeStamp systemTime;
    ASIOSamples samplePosition;
    double sampleRate;
    unsigned long flags;    // Time info flags
    char reserved[12];
};

struct ASIOTime {
    long reserved[4];
    AsioTimeInfo timeInfo;
    ASIOTimeCode timeCode;
};

// ASIOCallbacks - callback function pointers from host
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

/**
 * IASIO - The standard ASIO driver interface.
 *
 * This interface inherits from IUnknown and defines the 21 methods that
 * all ASIO drivers must implement. The order of virtual methods is critical
 * as ASIO hosts rely on the vtable layout.
 */
interface IASIO : public IUnknown
{
    // The following methods must be in this exact order for vtable compatibility
    virtual ASIOBool init(void* sysHandle) = 0;
    virtual void getDriverName(char* name) = 0;
    virtual long getDriverVersion() = 0;
    virtual void getErrorMessage(char* string) = 0;
    virtual ASIOError start() = 0;
    virtual ASIOError stop() = 0;
    virtual ASIOError getChannels(long* numInputChannels, long* numOutputChannels) = 0;
    virtual ASIOError getLatencies(long* inputLatency, long* outputLatency) = 0;
    virtual ASIOError getBufferSize(long* minSize, long* maxSize,
        long* preferredSize, long* granularity) = 0;
    virtual ASIOError canSampleRate(ASIOSampleRate sampleRate) = 0;
    virtual ASIOError getSampleRate(ASIOSampleRate* sampleRate) = 0;
    virtual ASIOError setSampleRate(ASIOSampleRate sampleRate) = 0;
    virtual ASIOError getClockSources(ASIOClockSource* clocks, long* numSources) = 0;
    virtual ASIOError setClockSource(long reference) = 0;
    virtual ASIOError getSamplePosition(ASIOSamples* sPos, ASIOTimeStamp* tStamp) = 0;
    virtual ASIOError getChannelInfo(ASIOChannelInfo* info) = 0;
    virtual ASIOError createBuffers(ASIOBufferInfo* bufferInfos, long numChannels,
        long bufferSize, ASIOCallbacks* callbacks) = 0;
    virtual ASIOError disposeBuffers() = 0;
    virtual ASIOError controlPanel() = 0;
    virtual ASIOError future(long selector, void* opt) = 0;
    virtual ASIOError outputReady() = 0;
};

// CLSID for AudioMatrix Virtual ASIO driver
// {A1B2C3D4-E5F6-7890-ABCD-EF1234567890}
DEFINE_GUID(CLSID_AudioMatrixASIO,
    0xA1B2C3D4, 0xE5F6, 0x7890,
    0xAB, 0xCD, 0xEF, 0x12, 0x34, 0x56, 0x78, 0x90);

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
 * AudioMatrix Virtual ASIO Driver implementation.
 *
 * CRITICAL: IASIO must be the FIRST base class to ensure correct vtable layout.
 * ASIO hosts cast the COM object directly to IASIO*, so the vtable pointer
 * must point to IASIO's vtable at offset 0.
 */
class VirtualAsioDriver : public IASIO
{
public:
    VirtualAsioDriver();
    virtual ~VirtualAsioDriver();

    // Prevent copying
    VirtualAsioDriver(const VirtualAsioDriver&) = delete;
    VirtualAsioDriver& operator=(const VirtualAsioDriver&) = delete;

    // IUnknown methods
    STDMETHOD(QueryInterface)(REFIID riid, void** ppv) override;
    STDMETHOD_(ULONG, AddRef)() override;
    STDMETHOD_(ULONG, Release)() override;

    // IASIO methods (must match interface order exactly)
    ASIOBool init(void* sysHandle) override;
    void getDriverName(char* name) override;
    long getDriverVersion() override;
    void getErrorMessage(char* string) override;
    ASIOError start() override;
    ASIOError stop() override;
    ASIOError getChannels(long* numInputChannels, long* numOutputChannels) override;
    ASIOError getLatencies(long* inputLatency, long* outputLatency) override;
    ASIOError getBufferSize(long* minSize, long* maxSize,
        long* preferredSize, long* granularity) override;
    ASIOError canSampleRate(ASIOSampleRate sampleRate) override;
    ASIOError getSampleRate(ASIOSampleRate* sampleRate) override;
    ASIOError setSampleRate(ASIOSampleRate sampleRate) override;
    ASIOError getClockSources(ASIOClockSource* clocks, long* numSources) override;
    ASIOError setClockSource(long reference) override;
    ASIOError getSamplePosition(ASIOSamples* sPos, ASIOTimeStamp* tStamp) override;
    ASIOError getChannelInfo(ASIOChannelInfo* info) override;
    ASIOError createBuffers(ASIOBufferInfo* bufferInfos, long numChannels,
        long bufferSize, ASIOCallbacks* callbacks) override;
    ASIOError disposeBuffers() override;
    ASIOError controlPanel() override;
    ASIOError future(long selector, void* opt) override;
    ASIOError outputReady() override;

private:
    // Shared memory management
    bool openSharedMemory(const std::wstring& name);
    void closeSharedMemory();

    // Audio processing thread
    void audioThreadProc();
    void processBuffers();

    // COM reference count
    std::atomic<ULONG> m_refCount{1};

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
 * Factory function to create the ASIO driver instance.
 * Called by DllGetClassObject.
 */
IASIO* CreateAudioMatrixDriver();

/**
 * Global driver instance and reference counting for COM.
 */
extern VirtualAsioDriver* g_driverInstance;
extern std::atomic<long> g_serverLockCount;

} // namespace audiomatrix
