# AudioMatrix E2E Tests

End-to-end tests for the AudioMatrix Web UI using Playwright.

## Prerequisites

- Node.js 18+
- A running AudioMatrix instance

## Setup

```bash
cd tests/e2e
npm install
npx playwright install chromium
```

## Running Tests

### Against local instance (localhost:8080)

```bash
npm test
```

### Against remote instance

```bash
BASE_URL=http://stagebox1.lan:8080 npm test
```

### With UI (interactive mode)

```bash
npm run test:ui
```

### With headed browser (see browser)

```bash
npm run test:headed
```

### Debug mode

```bash
npm run test:debug
```

## Test Structure

```
tests/e2e/
├── fixtures/
│   └── audiomatrix.ts    # Test utilities and fixtures
├── specs/
│   ├── smoke.spec.ts     # Basic connectivity and navigation
│   ├── devices.spec.ts   # Device attachment/detachment
│   ├── routing.spec.ts   # Routing matrix and route CRUD
│   └── websocket.spec.ts # WebSocket events and metering
├── playwright.config.ts   # Playwright configuration
├── package.json          # Dependencies
└── README.md             # This file
```

## Test Categories

### Smoke Tests (`smoke.spec.ts`)
- API health check
- Page navigation
- WebSocket connection
- Basic UI rendering

### Device Tests (`devices.spec.ts`)
- Device list display
- Attach/detach functionality
- Virtual device creation dialog
- Device meters

### Routing Tests (`routing.spec.ts`)
- Routing matrix display
- Route creation via API
- Route deletion via API
- Node/device selectors

### WebSocket Tests (`websocket.spec.ts`)
- WebSocket connection
- Event parsing (node_status, device_attached)
- Metering subscriptions
- Error handling

## Viewing Results

After running tests, view the HTML report:

```bash
npm run test:report
```

## CI Integration

Tests can be run in CI by setting the `CI` environment variable:

```bash
CI=true BASE_URL=http://audiomatrix:8080 npm test
```

This enables:
- Test retries on failure
- Single worker (sequential execution)
- Video recording on failure
