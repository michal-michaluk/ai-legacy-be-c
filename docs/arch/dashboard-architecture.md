# Dashboard Architecture

## Topic

Build the dashboard as a data-driven UI with explicit state, reusable
widgets, and an injectable broker-data boundary. Keep rendering independent
from live broker availability so the interface can represent loading, empty,
offline, and error states.

## How to implement

For a dashboard feature:

1. Define the widget's state and data shape.
2. Create a rendering boundary that accepts data and callbacks.
3. Keep broker API access in a data-fetching layer.
4. Normalize API data before passing it to charts or widgets.
5. Persist only intentional UI state such as layout or chart history.
6. Render loading, empty, success, offline, and error states explicitly.
7. Disable live timers and remote dependencies in headless/demo mode.
8. Exercise changed widgets through the `headless` mode with deterministic fixture data.

The main data flow should be:

```text
broker HTTP API
    -> fetch/parse layer
    -> normalized dashboard state
    -> status, listener, and chart widgets
    -> DOM and Chart.js rendering
```

## Code example

The dashboard state model keeps chart data separate from rendering:

```js
function createChartDataObject() {
  return {
    data: {
      rawData: [],
      smoothedData: [],
    },
    labels: {
      rawLabels: [],
      smoothedLabels: [],
    },
    options: {
      mustUpdate: false,
    },
  };
}

function createOptions() {
  return {
    chartDataType: "raw",
  };
}
```

`dashboard/src/app/dashboard.js` uses this shape, session storage, and a
`headless` mode. Preserve those seams when adding metrics or widgets. A
widget should consume normalized state instead of parsing an HTTP response
inside its render method.

## UI responsibilities

- `index.html`: page structure and stable element identity.
- `index.js`: browser entrypoint and page initialization.
- `dashboard.js`: dashboard state, polling, chart composition, and status.
- `sidebar.js`: navigation and sidebar behavior.
- `consts.js`: endpoint names and timing constants.
- `utils/`: reusable queue and browser helpers.
- Tailwind files: design tokens and utility styling.

Keep polling, session storage, chart transformations, and DOM updates
separable enough to run with deterministic fixtures.

## Best practices

- Use one normalized state model for live, fixture, and headless modes.
- Give every widget explicit loading, empty, offline, and error states.
- Keep endpoint URLs and timing values centralized.
- Bound chart history and storage; browser state must not grow forever.
- Use accessible labels and stable roles for operator workflows.
- Prefer CSS classes and design tokens over inline style duplication.
- Keep remote images and API failures non-fatal to unrelated dashboard state.

## What to avoid / NOGO

- Do not let every widget call the broker API independently.
- Do not make rendering depend on current time, random data, or remote assets.
- Do not silently treat malformed stored chart data as valid state.
- Do not use a live broker as the only way to develop or demonstrate a widget.
- Do not add a second state model only for stories or headless mode.
- Do not couple chart-specific data transformations to generic layout code.
