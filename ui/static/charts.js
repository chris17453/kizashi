// Vendored, dependency-free bar-chart renderer (ADR-0015). No external library, no CDN — this
// is an on-prem-capable enterprise product and must not depend on reaching an external network
// at runtime just to render a graph.
//
// Usage: <div class="chart" data-chart-source="chart-data-id"></div>
// paired with <script type="application/json" id="chart-data-id">{"labels":[...],"values":[...]}</script>
// The server renders the JSON (real data, already in the page); this only draws it.
(function () {
  "use strict";

  function bindTooltip(target, container, label, value) {
    var tooltip = container.querySelector(".chart-tooltip");
    if (!tooltip) {
      tooltip = document.createElement("div");
      tooltip.className = "chart-tooltip";
      tooltip.setAttribute("role", "status");
      tooltip.setAttribute("aria-live", "polite");
      container.appendChild(tooltip);
    }
    var show = function () {
      tooltip.textContent = String(label) + " · " + String(value);
      var targetRect = target.getBoundingClientRect();
      var containerRect = container.getBoundingClientRect();
      tooltip.style.left = (targetRect.left - containerRect.left + targetRect.width / 2) + "px";
      tooltip.style.top = (targetRect.top - containerRect.top - 6) + "px";
      tooltip.classList.add("visible");
    };
    var hide = function () { tooltip.classList.remove("visible"); };
    target.addEventListener("mouseenter", show);
    target.addEventListener("mouseleave", hide);
    target.addEventListener("focus", show);
    target.addEventListener("blur", hide);
  }

  function renderBarChart(container, data) {
    var labels = Array.isArray(data.labels) ? data.labels : [];
    var values = Array.isArray(data.values) ? data.values.map(function (value) {
      var numeric = Number(value);
      return Number.isFinite(numeric) && numeric >= 0 ? numeric : 0;
    }) : [];
    var max = Math.max.apply(null, values.concat([1]));
    var width = 720;
    var barHeight = 30;
    var gap = 12;
    var labelWidth = 178;
    var valueWidth = 58;
    var barMaxWidth = width - labelWidth - valueWidth;
    var hrefs = Array.isArray(data.hrefs) ? data.hrefs : [];
    var height = labels.length ? labels.length * (barHeight + gap) + gap : 74;

    var svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
    svg.setAttribute("viewBox", "0 0 " + width + " " + height);
    svg.setAttribute("width", "100%");
    svg.setAttribute("height", height);
    svg.setAttribute("role", "img");
    svg.setAttribute("class", "chart-svg");
    svg.setAttribute("aria-label", data.title || "bar chart");
    container.innerHTML = "";

    var defs = document.createElementNS("http://www.w3.org/2000/svg", "defs");
    var gradient = document.createElementNS("http://www.w3.org/2000/svg", "linearGradient");
    gradient.setAttribute("id", "chart-gradient-" + Math.random().toString(36).slice(2));
    gradient.setAttribute("x1", "0%"); gradient.setAttribute("x2", "100%");
    var start = document.createElementNS("http://www.w3.org/2000/svg", "stop");
    start.setAttribute("offset", "0%"); start.setAttribute("stop-color", "var(--accent-dim)");
    var end = document.createElementNS("http://www.w3.org/2000/svg", "stop");
    end.setAttribute("offset", "100%"); end.setAttribute("stop-color", "var(--accent)");
    gradient.appendChild(start); gradient.appendChild(end); defs.appendChild(gradient); svg.appendChild(defs);

    if (!labels.length) {
      var empty = document.createElementNS("http://www.w3.org/2000/svg", "text");
      empty.setAttribute("x", width / 2); empty.setAttribute("y", 42); empty.setAttribute("text-anchor", "middle");
      empty.setAttribute("class", "chart-empty"); empty.textContent = "No data in this scope"; svg.appendChild(empty);
      container.appendChild(svg); return;
    }

    [0, .25, .5, .75, 1].forEach(function (ratio) {
      var x = labelWidth + barMaxWidth * ratio;
      var line = document.createElementNS("http://www.w3.org/2000/svg", "line");
      line.setAttribute("x1", x); line.setAttribute("x2", x); line.setAttribute("y1", 4); line.setAttribute("y2", height - 4);
      line.setAttribute("class", "chart-grid-line"); svg.appendChild(line);
      var scale = document.createElementNS("http://www.w3.org/2000/svg", "text");
      scale.setAttribute("x", x); scale.setAttribute("y", height - 2); scale.setAttribute("text-anchor", "middle");
      scale.setAttribute("class", "chart-scale"); scale.textContent = String(Math.round(max * ratio)); svg.appendChild(scale);
    });

    for (var i = 0; i < labels.length; i++) {
      var y = gap + i * (barHeight + gap);
      var barWidth = max > 0 ? (values[i] / max) * barMaxWidth : 0;

      var label = document.createElementNS("http://www.w3.org/2000/svg", "text");
      label.setAttribute("x", labelWidth - 8);
      label.setAttribute("y", y + barHeight / 2 + 4);
      label.setAttribute("text-anchor", "end");
      label.setAttribute("class", "chart-label");
      label.textContent = labels[i];
      label.setAttribute("aria-label", String(labels[i]));
      svg.appendChild(label);

      var bar = document.createElementNS("http://www.w3.org/2000/svg", "rect");
      bar.setAttribute("x", labelWidth);
      bar.setAttribute("y", y);
      bar.setAttribute("width", Math.max(barWidth, 2));
      bar.setAttribute("height", barHeight);
      bar.setAttribute("rx", 4);
      bar.setAttribute("class", "chart-bar");
      bar.setAttribute("fill", "url(#" + gradient.getAttribute("id") + ")");
      bar.setAttribute("tabindex", "0");
      bar.setAttribute("role", "img");
      bar.setAttribute("aria-label", String(labels[i]) + ": " + String(values[i]));
      var title = document.createElementNS("http://www.w3.org/2000/svg", "title");
      title.textContent = String(labels[i]) + " · " + String(values[i]); bar.appendChild(title);
      bindTooltip(bar, container, labels[i], values[i]);
      var interactive = svg;
      if (hrefs[i]) {
        interactive = document.createElementNS("http://www.w3.org/2000/svg", "a");
        interactive.setAttribute("href", hrefs[i]);
        interactive.setAttribute("class", "chart-link");
        interactive.setAttribute("aria-label", "Inspect " + String(labels[i]));
        svg.appendChild(interactive);
      }
      interactive.appendChild(bar);

      var valueText = document.createElementNS("http://www.w3.org/2000/svg", "text");
      valueText.setAttribute("x", labelWidth + Math.min(barWidth, barMaxWidth) + 8);
      valueText.setAttribute("y", y + barHeight / 2 + 4);
      valueText.setAttribute("class", "chart-value");
      valueText.textContent = String(values[i]);
      interactive.appendChild(valueText);
    }

    container.appendChild(svg);
  }

  // A compact, dependency-free time-series renderer for the executive trend views.
  // Points remain links when the server provides hrefs, so the visual is an
  // investigation control rather than decoration.
  function renderLineChart(container, data) {
    var labels = Array.isArray(data.labels) ? data.labels : [];
    var values = Array.isArray(data.values) ? data.values.map(function (value) {
      var numeric = Number(value);
      return Number.isFinite(numeric) && numeric >= 0 ? numeric : 0;
    }) : [];
    var hrefs = Array.isArray(data.hrefs) ? data.hrefs : [];
    var width = 720;
    var height = 220;
    var pad = { top: 16, right: 18, bottom: 38, left: 42 };
    var plotWidth = width - pad.left - pad.right;
    var plotHeight = height - pad.top - pad.bottom;
    var max = Math.max.apply(null, values.concat([1]));
    var svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
    svg.setAttribute("viewBox", "0 0 " + width + " " + height);
    svg.setAttribute("width", "100%");
    svg.setAttribute("height", height);
    svg.setAttribute("role", "img");
    svg.setAttribute("class", "chart-svg chart-line-svg");
    svg.setAttribute("aria-label", data.title || "line chart");
    container.innerHTML = "";

    if (!labels.length) {
      var empty = document.createElementNS("http://www.w3.org/2000/svg", "text");
      empty.setAttribute("x", width / 2); empty.setAttribute("y", height / 2);
      empty.setAttribute("text-anchor", "middle"); empty.setAttribute("class", "chart-empty");
      empty.textContent = "No data in this scope"; svg.appendChild(empty);
      container.appendChild(svg); return;
    }

    var defs = document.createElementNS("http://www.w3.org/2000/svg", "defs");
    var gradient = document.createElementNS("http://www.w3.org/2000/svg", "linearGradient");
    var gradientId = "chart-area-" + Math.random().toString(36).slice(2);
    gradient.setAttribute("id", gradientId); gradient.setAttribute("x1", "0"); gradient.setAttribute("x2", "0"); gradient.setAttribute("y1", "0"); gradient.setAttribute("y2", "1");
    var stopTop = document.createElementNS("http://www.w3.org/2000/svg", "stop");
    stopTop.setAttribute("offset", "0%"); stopTop.setAttribute("stop-color", "var(--accent)"); stopTop.setAttribute("stop-opacity", ".32");
    var stopBottom = document.createElementNS("http://www.w3.org/2000/svg", "stop");
    stopBottom.setAttribute("offset", "100%"); stopBottom.setAttribute("stop-color", "var(--accent)"); stopBottom.setAttribute("stop-opacity", "0");
    gradient.appendChild(stopTop); gradient.appendChild(stopBottom); defs.appendChild(gradient); svg.appendChild(defs);

    [0, .25, .5, .75, 1].forEach(function (ratio) {
      var y = pad.top + plotHeight * (1 - ratio);
      var line = document.createElementNS("http://www.w3.org/2000/svg", "line");
      line.setAttribute("x1", pad.left); line.setAttribute("x2", width - pad.right); line.setAttribute("y1", y); line.setAttribute("y2", y); line.setAttribute("class", "chart-grid-line"); svg.appendChild(line);
      var scale = document.createElementNS("http://www.w3.org/2000/svg", "text");
      scale.setAttribute("x", pad.left - 8); scale.setAttribute("y", y + 4); scale.setAttribute("text-anchor", "end"); scale.setAttribute("class", "chart-scale"); scale.textContent = String(Math.round(max * ratio)); svg.appendChild(scale);
    });

    var point = function (index) {
      var x = labels.length === 1 ? pad.left + plotWidth / 2 : pad.left + (index * plotWidth / (labels.length - 1));
      var y = pad.top + plotHeight - ((values[index] || 0) / max * plotHeight);
      return { x: x, y: y };
    };
    var points = labels.map(function (_, index) { return point(index); });
    var linePath = points.map(function (item, index) { return (index ? "L" : "M") + item.x + " " + item.y; }).join(" ");
    var areaPath = linePath + " L" + points[points.length - 1].x + " " + (pad.top + plotHeight) + " L" + points[0].x + " " + (pad.top + plotHeight) + " Z";
    var area = document.createElementNS("http://www.w3.org/2000/svg", "path");
    area.setAttribute("d", areaPath); area.setAttribute("fill", "url(#" + gradientId + ")"); area.setAttribute("class", "chart-area"); svg.appendChild(area);
    var path = document.createElementNS("http://www.w3.org/2000/svg", "path");
    path.setAttribute("d", linePath); path.setAttribute("fill", "none"); path.setAttribute("class", "chart-line"); path.setAttribute("vector-effect", "non-scaling-stroke"); svg.appendChild(path);

    points.forEach(function (item, index) {
      var xLabel = document.createElementNS("http://www.w3.org/2000/svg", "text");
      xLabel.setAttribute("x", item.x); xLabel.setAttribute("y", height - 10); xLabel.setAttribute("text-anchor", "middle"); xLabel.setAttribute("class", "chart-label chart-line-label"); xLabel.textContent = labels[index]; svg.appendChild(xLabel);
      var target = svg;
      if (hrefs[index]) { target = document.createElementNS("http://www.w3.org/2000/svg", "a"); target.setAttribute("href", hrefs[index]); target.setAttribute("class", "chart-link"); target.setAttribute("aria-label", "Inspect " + String(labels[index])); svg.appendChild(target); }
      var circle = document.createElementNS("http://www.w3.org/2000/svg", "circle");
      circle.setAttribute("cx", item.x); circle.setAttribute("cy", item.y); circle.setAttribute("r", "5"); circle.setAttribute("class", "chart-point"); circle.setAttribute("tabindex", "0"); circle.setAttribute("role", "img"); circle.setAttribute("aria-label", String(labels[index]) + ": " + String(values[index]));
      var title = document.createElementNS("http://www.w3.org/2000/svg", "title"); title.textContent = String(labels[index]) + " · " + String(values[index]); circle.appendChild(title); target.appendChild(circle);
      bindTooltip(circle, container, labels[index], values[index]);
    });
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
        if (container.getAttribute("data-chart-kind") === "line") renderLineChart(container, data);
        else renderBarChart(container, data);
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
