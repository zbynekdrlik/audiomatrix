/**
 * AudioMatrix Virtual ASIO Driver - Shared Memory IPC
 *
 * Handles inter-process communication with AudioMatrix service
 * using Windows shared memory for lock-free audio transfer.
 */

#include "virtual_asio.h"
#include <cstring>

namespace audiomatrix {

/**
 * Lock-free ring buffer for audio transfer.
 * Uses atomic read/write positions for thread-safe operation.
 */
class AudioRingBuffer {
public:
    AudioRingBuffer(float* buffer, size_t capacity,
                    std::atomic<uint64_t>& readPos,
                    std::atomic<uint64_t>& writePos)
        : m_buffer(buffer)
        , m_capacity(capacity)
        , m_readPos(readPos)
        , m_writePos(writePos)
    {}

    /**
     * Write samples to the ring buffer.
     * Returns number of samples actually written.
     */
    size_t write(const float* data, size_t count) {
        uint64_t read = m_readPos.load(std::memory_order_acquire);
        uint64_t write = m_writePos.load(std::memory_order_relaxed);

        size_t available = m_capacity - (write - read);
        size_t toWrite = std::min(count, available);

        if (toWrite == 0) return 0;

        size_t writeIdx = write % m_capacity;
        size_t firstPart = std::min(toWrite, m_capacity - writeIdx);
        size_t secondPart = toWrite - firstPart;

        std::memcpy(m_buffer + writeIdx, data, firstPart * sizeof(float));
        if (secondPart > 0) {
            std::memcpy(m_buffer, data + firstPart, secondPart * sizeof(float));
        }

        m_writePos.store(write + toWrite, std::memory_order_release);
        return toWrite;
    }

    /**
     * Read samples from the ring buffer.
     * Returns number of samples actually read.
     */
    size_t read(float* data, size_t count) {
        uint64_t read = m_readPos.load(std::memory_order_relaxed);
        uint64_t write = m_writePos.load(std::memory_order_acquire);

        size_t available = write - read;
        size_t toRead = std::min(count, available);

        if (toRead == 0) return 0;

        size_t readIdx = read % m_capacity;
        size_t firstPart = std::min(toRead, m_capacity - readIdx);
        size_t secondPart = toRead - firstPart;

        std::memcpy(data, m_buffer + readIdx, firstPart * sizeof(float));
        if (secondPart > 0) {
            std::memcpy(data + firstPart, m_buffer, secondPart * sizeof(float));
        }

        m_readPos.store(read + toRead, std::memory_order_release);
        return toRead;
    }

    /**
     * Get number of samples available to read.
     */
    size_t available() const {
        uint64_t read = m_readPos.load(std::memory_order_acquire);
        uint64_t write = m_writePos.load(std::memory_order_acquire);
        return write - read;
    }

    /**
     * Get remaining space for writing.
     */
    size_t space() const {
        uint64_t read = m_readPos.load(std::memory_order_acquire);
        uint64_t write = m_writePos.load(std::memory_order_acquire);
        return m_capacity - (write - read);
    }

private:
    float* m_buffer;
    size_t m_capacity;
    std::atomic<uint64_t>& m_readPos;
    std::atomic<uint64_t>& m_writePos;
};

/**
 * Shared memory manager for AudioMatrix IPC.
 * Creates or opens shared memory regions and manages ring buffers.
 */
class SharedMemoryManager {
public:
    static constexpr wchar_t BASE_NAME[] = L"Local\\AudioMatrix_VASIO_";
    static constexpr size_t DEFAULT_RING_SIZE = 65536; // 64K samples

    SharedMemoryManager() = default;
    ~SharedMemoryManager() { close(); }

    /**
     * Create shared memory for a virtual device.
     * Called by AudioMatrix service when creating a virtual device.
     */
    bool create(const std::wstring& deviceId, uint32_t channels,
                uint32_t sampleRate, uint32_t bufferSize) {

        std::wstring name = std::wstring(BASE_NAME) + deviceId;

        size_t ringSize = DEFAULT_RING_SIZE * channels * sizeof(float);
        size_t totalSize = sizeof(SharedMemoryHeader) + ringSize * 2;

        m_handle = CreateFileMappingW(
            INVALID_HANDLE_VALUE,
            nullptr,
            PAGE_READWRITE,
            0,
            static_cast<DWORD>(totalSize),
            name.c_str()
        );

        if (!m_handle) {
            return false;
        }

        m_ptr = MapViewOfFile(m_handle, FILE_MAP_ALL_ACCESS, 0, 0, totalSize);
        if (!m_ptr) {
            CloseHandle(m_handle);
            m_handle = nullptr;
            return false;
        }

        // Initialize header
        auto* header = static_cast<SharedMemoryHeader*>(m_ptr);
        header->magic = SHARED_MEMORY_MAGIC;
        header->version = SHARED_MEMORY_VERSION;
        header->channels = channels;
        header->sample_rate = sampleRate;
        header->buffer_size = bufferSize;
        header->read_pos = 0;
        header->write_pos = 0;
        header->flags = FLAG_SERVICE_CONNECTED;
        std::memset(header->reserved, 0, sizeof(header->reserved));

        m_size = totalSize;
        m_header = header;

        return true;
    }

    /**
     * Open existing shared memory.
     * Called by ASIO driver to connect to AudioMatrix service.
     */
    bool open(const std::wstring& deviceId) {
        std::wstring name = std::wstring(BASE_NAME) + deviceId;

        m_handle = OpenFileMappingW(FILE_MAP_ALL_ACCESS, FALSE, name.c_str());
        if (!m_handle) {
            return false;
        }

        m_ptr = MapViewOfFile(m_handle, FILE_MAP_ALL_ACCESS, 0, 0, 0);
        if (!m_ptr) {
            CloseHandle(m_handle);
            m_handle = nullptr;
            return false;
        }

        auto* header = static_cast<SharedMemoryHeader*>(m_ptr);
        if (header->magic != SHARED_MEMORY_MAGIC) {
            UnmapViewOfFile(m_ptr);
            CloseHandle(m_handle);
            m_ptr = nullptr;
            m_handle = nullptr;
            return false;
        }

        m_header = header;
        return true;
    }

    void close() {
        if (m_ptr) {
            UnmapViewOfFile(m_ptr);
            m_ptr = nullptr;
        }
        if (m_handle) {
            CloseHandle(m_handle);
            m_handle = nullptr;
        }
        m_header = nullptr;
    }

    SharedMemoryHeader* header() { return m_header; }
    bool isOpen() const { return m_handle != nullptr; }

private:
    HANDLE m_handle = nullptr;
    void* m_ptr = nullptr;
    size_t m_size = 0;
    SharedMemoryHeader* m_header = nullptr;
};

} // namespace audiomatrix
