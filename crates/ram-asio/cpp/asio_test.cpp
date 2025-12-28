/**
 * AudioMatrix ASIO Driver Test Tool
 *
 * Tests the ASIO driver by:
 * 1. Loading and initializing the driver
 * 2. Querying capabilities (channels, sample rates, buffer sizes)
 * 3. Creating buffers and starting audio
 * 4. Verifying callbacks are called
 * 5. Stopping and disposing buffers
 *
 * Exit codes:
 *   0 = All tests passed
 *   1 = Driver load/init failed
 *   2 = Capability query failed
 *   3 = Buffer creation failed
 *   4 = Audio callback test failed
 */

#include "virtual_asio.h"
#include <cstdio>
#include <atomic>
#include <chrono>
#include <thread>

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
        case 1: // kAsioSelectorSupported
            return 1; // We support message queries
        case 4: // kAsioSupportsTimeInfo
            return 0; // We don't use bufferSwitchTimeInfo
        default:
            return 0;
    }
}

static ASIOTime* bufferSwitchTimeInfo(ASIOTime* params, long doubleBufferIndex, ASIOBool directProcess) {
    (void)doubleBufferIndex;
    (void)directProcess;
    return params;
}

static ASIOCallbacks g_callbacks = {
    bufferSwitch,
    sampleRateDidChange,
    asioMessage,
    bufferSwitchTimeInfo
};

int main() {
    printf("=== AudioMatrix ASIO Driver Test ===\n\n");

    // Create driver instance directly (we're in the same process)
    printf("[1] Creating driver instance...\n");
    IASIO* driver = audiomatrix::CreateAudioMatrixDriver();
    if (!driver) {
        printf("FAIL: Could not create driver instance\n");
        return 1;
    }
    printf("  OK: Driver instance created\n");

    // Initialize
    printf("[2] Initializing driver...\n");
    ASIOBool initResult = driver->init(nullptr);
    if (initResult != ASIOTrue) {
        char errorMsg[128] = {0};
        driver->getErrorMessage(errorMsg);
        printf("FAIL: init() returned false - %s\n", errorMsg);
        driver->Release();
        return 1;
    }
    printf("  OK: Driver initialized\n");

    // Get driver info
    char driverName[32] = {0};
    driver->getDriverName(driverName);
    long driverVersion = driver->getDriverVersion();
    printf("  Driver: %s v%ld\n", driverName, driverVersion);

    // Query channels
    printf("[3] Querying capabilities...\n");
    long numInputChannels = 0, numOutputChannels = 0;
    ASIOError err = driver->getChannels(&numInputChannels, &numOutputChannels);
    if (err != ASE_OK) {
        printf("FAIL: getChannels() returned %ld\n", err);
        driver->Release();
        return 2;
    }
    printf("  Channels: %ld in, %ld out\n", numInputChannels, numOutputChannels);

    // Query buffer sizes
    long minSize = 0, maxSize = 0, preferredSize = 0, granularity = 0;
    err = driver->getBufferSize(&minSize, &maxSize, &preferredSize, &granularity);
    if (err != ASE_OK) {
        printf("FAIL: getBufferSize() returned %ld\n", err);
        driver->Release();
        return 2;
    }
    printf("  Buffer size: min=%ld, max=%ld, preferred=%ld\n", minSize, maxSize, preferredSize);

    // Query sample rate
    ASIOSampleRate sampleRate = 0;
    err = driver->getSampleRate(&sampleRate);
    if (err != ASE_OK) {
        printf("FAIL: getSampleRate() returned %ld\n", err);
        driver->Release();
        return 2;
    }
    printf("  Sample rate: %.0f Hz\n", sampleRate);

    // Test sample rate support
    err = driver->canSampleRate(48000.0);
    printf("  48kHz supported: %s\n", err == ASE_OK ? "yes" : "no");

    // Get channel info
    ASIOChannelInfo channelInfo = {0};
    channelInfo.channel = 0;
    channelInfo.isInput = ASIOFalse;
    err = driver->getChannelInfo(&channelInfo);
    if (err != ASE_OK) {
        printf("FAIL: getChannelInfo() returned %ld\n", err);
        driver->Release();
        return 2;
    }
    printf("  Channel 0 out: type=%ld, name=%s\n", channelInfo.type, channelInfo.name);
    printf("  OK: All capability queries passed\n");

    // Create buffers
    printf("[4] Creating buffers...\n");
    long bufferSize = preferredSize;

    // Create buffer info for 2 inputs and 2 outputs
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

    err = driver->createBuffers(bufferInfos, 4, bufferSize, &g_callbacks);
    if (err != ASE_OK) {
        printf("FAIL: createBuffers() returned %ld\n", err);
        driver->Release();
        return 3;
    }

    // Verify buffer pointers
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
        return 3;
    }
    printf("  OK: Buffers created (size=%ld samples)\n", bufferSize);

    // Start audio
    printf("[5] Starting audio...\n");
    err = driver->start();
    if (err != ASE_OK) {
        printf("FAIL: start() returned %ld\n", err);
        driver->disposeBuffers();
        driver->Release();
        return 4;
    }
    printf("  OK: Audio started\n");

    // Wait for callbacks
    printf("[6] Testing audio callbacks...\n");
    auto startTime = std::chrono::steady_clock::now();
    auto timeout = std::chrono::seconds(3);

    while (!g_testComplete) {
        std::this_thread::sleep_for(std::chrono::milliseconds(10));
        auto elapsed = std::chrono::steady_clock::now() - startTime;
        if (elapsed > timeout) {
            break;
        }
    }

    int callbackCount = g_bufferSwitchCount.load();
    printf("  Buffer switch callbacks received: %d\n", callbackCount);

    if (callbackCount < 10) {
        printf("FAIL: Expected at least 10 callbacks, got %d\n", callbackCount);
        driver->stop();
        driver->disposeBuffers();
        driver->Release();
        return 4;
    }
    printf("  OK: Audio callbacks working\n");

    // Get sample position
    ASIOSamples samplePos = 0;
    ASIOTimeStamp timeStamp = 0;
    err = driver->getSamplePosition(&samplePos, &timeStamp);
    if (err == ASE_OK) {
        printf("  Sample position: %lld\n", samplePos);
    }

    // Stop audio
    printf("[7] Stopping audio...\n");
    err = driver->stop();
    if (err != ASE_OK) {
        printf("WARN: stop() returned %ld\n", err);
    }
    printf("  OK: Audio stopped\n");

    // Dispose buffers
    printf("[8] Disposing buffers...\n");
    err = driver->disposeBuffers();
    if (err != ASE_OK) {
        printf("WARN: disposeBuffers() returned %ld\n", err);
    }
    printf("  OK: Buffers disposed\n");

    // Release driver
    printf("[9] Releasing driver...\n");
    driver->Release();
    printf("  OK: Driver released\n");

    printf("\n=== ALL TESTS PASSED ===\n");
    return 0;
}
