// Vendored, dependency-free bar-chart renderer (ADR-0015). No external library, no CDN — this
// is an on-prem-capable enterprise product and must not depend on reaching an external network
// at runtime just to render a graph.
//
// Usage: <div class="chart" data-chart-source="chart-data-id"></div>
// paired with <script type="application/json" id="chart-data-id">{"labels":[...],"values":[...]}</script>
// The server renders the JSON (real data, already in the page); this only draws it.
(function () {
  "use strict";

  function renderBarChart(container, data) {
    var labels = data.labels || [];
    var values = data.values || [];
    var max = Math.max.apply(null, values.concat([1]));
    var width = 640;
    var barHeight = 28;
    var gap = 10;
    var labelWidth = 160;
    var height = labels.length * (barHeight + gap) + gap;

    var svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
    svg.setAttribute("viewBox", "0 0 " + width + " " + height);
    svg.setAttribute("width", "100%");
    svg.setAttribute("height", height);
    svg.setAttribute("role", "img");
    svg.setAttribute("aria-label", data.title || "bar chart");

    for (var i = 0; i < labels.length; i++) {
      var y = gap + i * (barHeight + gap);
      var barMaxWidth = width - labelWidth - 60;
      var barWidth = max > 0 ? (values[i] / max) * barMaxWidth : 0;

      var label = document.createElementNS("http://www.w3.org/2000/svg", "text");
      label.setAttribute("x", labelWidth - 8);
      label.setAttribute("y", y + barHeight / 2 + 4);
      label.setAttribute("text-anchor", "end");
      label.setAttribute("class", "chart-label");
      label.textContent = labels[i];
      svg.appendChild(label);

      var bar = document.createElementNS("http://www.w3.org/2000/svg", "rect");
      bar.setAttribute("x", labelWidth);
      bar.setAttribute("y", y);
      bar.setAttribute("width", Math.max(barWidth, 1));
      bar.setAttribute("height", barHeight);
      bar.setAttribute("rx", 4);
      bar.setAttribute("class", "chart-bar");
      svg.appendChild(bar);

      var valueText = document.createElementNS("http://www.w3.org/2000/svg", "text");
      valueText.setAttribute("x", labelWidth + barWidth + 8);
      valueText.setAttribute("y", y + barHeight / 2 + 4);
      valueText.setAttribute("class", "chart-value");
      valueText.textContent = String(values[i]);
      svg.appendChild(valueText);
    }

    container.innerHTML = "";
    container.appendChild(svg);
  }

  function renderAll() {
    var charts = document.querySelectorAll("[data-chart-source]");
    for (var i = 0; i < charts.length; i++) {
      var container = charts[i];
      var sourceId = container.getAttribute("data-chart-source");
      var sourceEl = document.getElementById(sourceId);
      if (!sourceEl) continue;
      try {
        var data = JSON.parse(sourceEl.textContent);
        renderBarChart(container, data);
      } catch (e) {
        // A chart that fails to render must never take the rest of the page down with it —
        // the underlying table (server-rendered, ADR-0015) is still there and still correct.
        console.error("chart render failed", e);
      }
    }
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", renderAll);
  } else {
    renderAll();
  }
})();
