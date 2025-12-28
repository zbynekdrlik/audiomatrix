/**
 * AudioMatrix Virtual ASIO Driver - Main Implementation
 *
 * Implements the ASIO interface for virtual audio device routing.
 * Communicates with AudioMatrix service via shared memory.
 */

#include "virtual_asio.h"
#include <cstring>
#include <algorithm>
#include <chrono>
#include <cstdio>

// Debug logging - writes to a file so we can see what's happening
static FILE* g_debugLog = nullptr;

static void DebugLog(const char* fmt, ...) {
    if (!g_debugLog) {
        g_debugLog = fopen("C:\\AudioMatrix_debug.log", "a");
    }
    if (g_debugLog) {
        va_list args;
        va_start(args, fmt);
        vfprintf(g_debugLog, fmt, args);
        fprintf(g_debugLog, "\n");
        fflush(g_debugLog);
        va_end(args);
    }
}

namespace audiomatrix {

// Global instance for COM
VirtualAsioDriver* g_driverInstance = nullptr;
std::atomic<long> g_serverLockCount{0};

IASIO* CreateAudioMatrixDriver() {
    DebugLog("CreateAudioMatrixDriver called");
    if (!g_driverInstance) {
        DebugLog("Creating new VirtualAsioDriver");
        g_driverInstance = new VirtualAsioDriver();
    }
    g_driverInstance->AddRef();
    DebugLog("Returning IASIO* at %p", static_cast<IASIO*>(g_driverInstance));
    return static_cast<IASIO*>(g_driverInstance);
}

VirtualAsioDriver::VirtualAsioDriver() {
    m_bufferEvent = CreateEventW(nullptr, FALSE, FALSE, nullptr);
}

VirtualAsioDriver::~VirtualAsioDriver() {
    stop();
    disposeBuffers();
    closeSharedMemory();

    if (m_bufferEvent) {
        CloseHandle(m_bufferEvent);
        m_bufferEvent = nullptr;
    }
}

// IUnknown implementation
STDMETHODIMP VirtualAsioDriver::QueryInterface(REFIID riid, void** ppv) {
    DebugLog("QueryInterface called");
    if (!ppv) {
        DebugLog("QueryInterface: ppv is null");
        return E_POINTER;
    }

    *ppv = nullptr;

    if (IsEqualIID(riid, IID_IUnknown) || IsEqualIID(riid, CLSID_AudioMatrixASIO)) {
        *ppv = static_cast<IASIO*>(this);
        AddRef();
        DebugLog("QueryInterface: returning IASIO* at %p", *ppv);
        return S_OK;
    }

    DebugLog("QueryInterface: E_NOINTERFACE");
    return E_NOINTERFACE;
}

STDMETHODIMP_(ULONG) VirtualAsioDriver::AddRef() {
    return ++m_refCount;
}

STDMETHODIMP_(ULONG) VirtualAsioDriver::Release() {
    ULONG count = --m_refCount;
    if (count == 0) {
        g_driverInstance = nullptr;
        delete this;
    }
    return count;
}

// IASIO implementation
ASIOBool VirtualAsioDriver::init(void* sysHandle) {
    DebugLog("init called with sysHandle=%p", sysHandle);
    if (m_initialized) {
        DebugLog("init: already initialized");
        return ASIOTrue;
    }

    // Try to connect to AudioMatrix service via shared memory
    std::wstring sharedMemName = L"Local\\AudioMatrix_VASIO_Default";
    if (!openSharedMemory(sharedMemName)) {
        // Service not running or no virtual device configured
        // Use default configuration and wait for service
        strcpy_s(m_errorMessage, "AudioMatrix service not connected");
        DebugLog("init: service not connected");
    }

    m_initialized = true;
    DebugLog("init: returning ASIOTrue");
    return ASIOTrue;
}

void VirtualAsioDriver::getDriverName(char* name) {
    if (name) {
        strcpy_s(name, 32, "AudioMatrix Virtual");
    }
}

long VirtualAsioDriver::getDriverVersion() {
    return 1; // Version 1.0
}

void VirtualAsioDriver::getErrorMessage(char* string) {
    if (string) {
        strcpy_s(string, 128, m_errorMessage);
    }
}

ASIOError VirtualAsioDriver::start() {
    if (!m_initialized) {
        return ASE_NotPresent;
    }

    if (m_running) {
        return ASE_OK;
    }

    if (!m_callbacks) {
        strcpy_s(m_errorMessage, "No callbacks registered");
        return ASE_InvalidMode;
    }

    m_stopRequested = false;
    m_running = true;
    m_samplePosition = 0;

    // Update shared memory flags
    if (m_header) {
        m_header->flags |= FLAG_DRIVER_RUNNING;
    }

    // Start audio processing thread
    m_audioThread = std::thread(&VirtualAsioDriver::audioThreadProc, this);

    return ASE_OK;
}

ASIOError VirtualAsioDriver::stop() {
    if (!m_running) {
        return ASE_OK;
    }

    m_stopRequested = true;

    // Signal the buffer event to wake up the thread
    if (m_bufferEvent) {
        SetEvent(m_bufferEvent);
    }

    // Wait for audio thread to finish
    if (m_audioThread.joinable()) {
        m_audioThread.join();
    }

    m_running = false;

    // Update shared memory flags
    if (m_header) {
        m_header->flags &= ~FLAG_DRIVER_RUNNING;
    }

    return ASE_OK;
}

ASIOError VirtualAsioDriver::getChannels(long* numInputChannels, long* numOutputChannels) {
    DebugLog("getChannels called");
    if (numInputChannels) *numInputChannels = m_numInputChannels;
    if (numOutputChannels) *numOutputChannels = m_numOutputChannels;
    DebugLog("getChannels: %ld in, %ld out", m_numInputChannels, m_numOutputChannels);
    return ASE_OK;
}

ASIOError VirtualAsioDriver::getLatencies(long* inputLatency, long* outputLatency) {
    // Latency is primarily the buffer size
    if (inputLatency) *inputLatency = m_bufferSize;
    if (outputLatency) *outputLatency = m_bufferSize;
    return ASE_OK;
}

ASIOError VirtualAsioDriver::getBufferSize(long* minSize, long* maxSize,
                                           long* preferredSize, long* granularity) {
    if (minSize) *minSize = 64;
    if (maxSize) *maxSize = 4096;
    if (preferredSize) *preferredSize = 256;
    if (granularity) *granularity = 0; // Power of 2 only
    return ASE_OK;
}

ASIOError VirtualAsioDriver::canSampleRate(ASIOSampleRate sampleRate) {
    // Support common sample rates
    if (sampleRate == 44100.0 || sampleRate == 48000.0 ||
        sampleRate == 88200.0 || sampleRate == 96000.0 ||
        sampleRate == 176400.0 || sampleRate == 192000.0) {
        return ASE_OK;
    }
    return ASE_NoClock;
}

ASIOError VirtualAsioDriver::getSampleRate(ASIOSampleRate* sampleRate) {
    if (sampleRate) *sampleRate = m_sampleRate;
    return ASE_OK;
}

ASIOError VirtualAsioDriver::setSampleRate(ASIOSampleRate sampleRate) {
    ASIOError err = canSampleRate(sampleRate);
    if (err != ASE_OK) {
        return err;
    }

    m_sampleRate = sampleRate;

    // Update shared memory if connected
    if (m_header) {
        m_header->sample_rate = static_cast<uint32_t>(sampleRate);
    }

    return ASE_OK;
}

ASIOError VirtualAsioDriver::getClockSources(ASIOClockSource* clocks, long* numSources) {
    if (numSources) *numSources = 1;
    if (clocks) {
        clocks[0].index = 0;
        clocks[0].associatedChannel = -1;
        clocks[0].associatedGroup = -1;
        clocks[0].isCurrentSource = ASIOTrue;
        strcpy_s(clocks[0].name, 32, "Internal");
    }
    return ASE_OK;
}

ASIOError VirtualAsioDriver::setClockSource(long /*reference*/) {
    // Only internal clock supported
    return ASE_OK;
}

ASIOError VirtualAsioDriver::getSamplePosition(ASIOSamples* sPos, ASIOTimeStamp* tStamp) {
    if (sPos) *sPos = m_samplePosition;
    if (tStamp) {
        auto now = std::chrono::high_resolution_clock::now();
        auto nanos = std::chrono::duration_cast<std::chrono::nanoseconds>(
            now.time_since_epoch()).count();
        *tStamp = nanos;
    }
    return ASE_OK;
}

ASIOError VirtualAsioDriver::getChannelInfo(ASIOChannelInfo* info) {
    if (!info) return ASE_InvalidParameter;

    long channel = info->channel;
    bool isInput = info->isInput != 0;

    long maxChannel = isInput ? m_numInputChannels : m_numOutputChannels;
    if (channel < 0 || channel >= maxChannel) {
        return ASE_InvalidParameter;
    }

    info->isActive = ASIOFalse;
    info->channelGroup = 0;
    info->type = ASIOSTFloat32LSB; // 32-bit float, little-endian

    // Check if this channel is in our active buffers
    for (const auto& bufInfo : m_bufferInfos) {
        if (bufInfo.channelNum == channel &&
            (bufInfo.isInput != 0) == isInput) {
            info->isActive = ASIOTrue;
            break;
        }
    }

    // Generate channel name
    const char* prefix = isInput ? "In" : "Out";
    snprintf(info->name, 32, "%s %ld", prefix, channel + 1);

    return ASE_OK;
}

ASIOError VirtualAsioDriver::createBuffers(ASIOBufferInfo* bufferInfos, long numChannels,
                                           long bufferSize, ASIOCallbacks* callbacks) {
    DebugLog("createBuffers called: numChannels=%ld, bufferSize=%ld, callbacks=%p",
             numChannels, bufferSize, (void*)callbacks);
    if (!bufferInfos || !callbacks || numChannels <= 0 || bufferSize <= 0) {
        DebugLog("createBuffers: invalid parameters");
        return ASE_InvalidParameter;
    }

    // Dispose existing buffers
    disposeBuffers();

    m_bufferSize = bufferSize;
    m_callbacks = callbacks;

    // Update shared memory
    if (m_header) {
        m_header->buffer_size = static_cast<uint32_t>(bufferSize);
    }

    // Allocate double buffers for each channel
    m_inputBuffers[0].clear();
    m_inputBuffers[1].clear();
    m_outputBuffers[0].clear();
    m_outputBuffers[1].clear();

    m_bufferInfos.resize(static_cast<size_t>(numChannels));

    for (long i = 0; i < numChannels; ++i) {
        m_bufferInfos[static_cast<size_t>(i)] = bufferInfos[i];

        if (bufferInfos[i].isInput) {
            m_inputBuffers[0].emplace_back(static_cast<size_t>(bufferSize), 0.0f);
            m_inputBuffers[1].emplace_back(static_cast<size_t>(bufferSize), 0.0f);
            bufferInfos[i].buffers[0] = m_inputBuffers[0].back().data();
            bufferInfos[i].buffers[1] = m_inputBuffers[1].back().data();
        } else {
            m_outputBuffers[0].emplace_back(static_cast<size_t>(bufferSize), 0.0f);
            m_outputBuffers[1].emplace_back(static_cast<size_t>(bufferSize), 0.0f);
            bufferInfos[i].buffers[0] = m_outputBuffers[0].back().data();
            bufferInfos[i].buffers[1] = m_outputBuffers[1].back().data();
        }
    }

    return ASE_OK;
}

ASIOError VirtualAsioDriver::disposeBuffers() {
    if (m_running) {
        stop();
    }

    m_bufferInfos.clear();
    m_inputBuffers[0].clear();
    m_inputBuffers[1].clear();
    m_outputBuffers[0].clear();
    m_outputBuffers[1].clear();
    m_callbacks = nullptr;

    return ASE_OK;
}

ASIOError VirtualAsioDriver::controlPanel() {
    // Could open a configuration dialog
    // For now, just return OK
    return ASE_OK;
}

ASIOError VirtualAsioDriver::future(long /*selector*/, void* /*opt*/) {
    return ASE_NotPresent;
}

ASIOError VirtualAsioDriver::outputReady() {
    // We support this optimization
    return ASE_OK;
}

bool VirtualAsioDriver::openSharedMemory(const std::wstring& name) {
    // Calculate required size
    size_t ringBufferSize = static_cast<size_t>(m_bufferSize) *
                            static_cast<size_t>(std::max(m_numInputChannels, m_numOutputChannels)) *
                            sizeof(float) * 8; // 8x buffer for ring buffer headroom
    m_sharedMemSize = sizeof(SharedMemoryHeader) + ringBufferSize * 2; // Input + output

    // Try to open existing shared memory (created by AudioMatrix service)
    m_sharedMemHandle = OpenFileMappingW(FILE_MAP_ALL_ACCESS, FALSE, name.c_str());

    if (!m_sharedMemHandle) {
        // Service not running yet - we'll operate without shared memory
        return false;
    }

    m_sharedMemPtr = MapViewOfFile(m_sharedMemHandle, FILE_MAP_ALL_ACCESS, 0, 0, 0);
    if (!m_sharedMemPtr) {
        CloseHandle(m_sharedMemHandle);
        m_sharedMemHandle = nullptr;
        return false;
    }

    m_header = static_cast<SharedMemoryHeader*>(m_sharedMemPtr);

    // Validate magic number
    if (m_header->magic != SHARED_MEMORY_MAGIC) {
        UnmapViewOfFile(m_sharedMemPtr);
        CloseHandle(m_sharedMemHandle);
        m_sharedMemPtr = nullptr;
        m_sharedMemHandle = nullptr;
        m_header = nullptr;
        return false;
    }

    // Set up buffer pointers
    auto* data = static_cast<uint8_t*>(m_sharedMemPtr);
    m_inputBuffer = reinterpret_cast<float*>(data + sizeof(SharedMemoryHeader));
    m_outputBuffer = reinterpret_cast<float*>(data + sizeof(SharedMemoryHeader) + ringBufferSize);

    // Read configuration from shared memory
    m_numInputChannels = static_cast<long>(m_header->channels);
    m_numOutputChannels = static_cast<long>(m_header->channels);
    m_sampleRate = static_cast<double>(m_header->sample_rate);
    m_bufferSize = static_cast<long>(m_header->buffer_size);

    return true;
}

void VirtualAsioDriver::closeSharedMemory() {
    if (m_sharedMemPtr) {
        UnmapViewOfFile(m_sharedMemPtr);
        m_sharedMemPtr = nullptr;
    }
    if (m_sharedMemHandle) {
        CloseHandle(m_sharedMemHandle);
        m_sharedMemHandle = nullptr;
    }
    m_header = nullptr;
    m_inputBuffer = nullptr;
    m_outputBuffer = nullptr;
}

void VirtualAsioDriver::audioThreadProc() {
    // Set thread priority for real-time audio
    SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_TIME_CRITICAL);

    // Calculate timer interval in milliseconds
    double bufferDurationMs = (static_cast<double>(m_bufferSize) / m_sampleRate) * 1000.0;
    DWORD timerInterval = static_cast<DWORD>(std::max(1.0, bufferDurationMs * 0.9));

    while (!m_stopRequested) {
        // Wait for next buffer period
        WaitForSingleObject(m_bufferEvent, timerInterval);

        if (m_stopRequested) break;

        processBuffers();
    }
}

void VirtualAsioDriver::processBuffers() {
    // Switch buffer index
    m_currentBuffer = 1 - m_currentBuffer;

    // If connected to AudioMatrix, transfer audio via shared memory
    if (m_header && m_inputBuffer && m_outputBuffer) {
        // Read input samples from shared memory (what AudioMatrix sends us)
        // This would be audio from network sources, other devices, etc.

        // Write output samples to shared memory (what we send to AudioMatrix)
        // This is audio from the DAW

        // For now, just copy silence to inputs and read outputs
        // Full implementation requires proper ring buffer management
    }

    // Update sample position
    m_samplePosition += m_bufferSize;

    // Call the ASIO host callback
    if (m_callbacks && m_callbacks->bufferSwitch) {
        m_callbacks->bufferSwitch(m_currentBuffer, ASIOTrue);
    }
}

} // namespace audiomatrix
