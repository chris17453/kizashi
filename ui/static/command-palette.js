(function () {
  'use strict';
  var launcher = document.getElementById('command-launcher');
  var palette = document.getElementById('command-palette');
  var input = document.getElementById('command-input');
  var empty = document.getElementById('command-empty');
  var liveResults = document.getElementById('command-live-results');
  if (!launcher || !palette || !input) return;
  var items = Array.prototype.slice.call(palette.querySelectorAll('[data-command-item]'));
  var liveItems = [];
  var lastFocus = null;
  var shortcutTimer = null;
  var searchTimer = null;
  var searchSequence = 0;
  var shortcutMap = {};
  items.forEach(function (item) {
    var key = item.querySelector('kbd');
    if (key) shortcutMap[key.textContent.trim().toLowerCase()] = item.getAttribute('href');
  });
  function allItems() { return items.concat(liveItems); }
  function visibleItems() { return allItems().filter(function (item) { return !item.hidden; }); }
  function updateEmpty() {
    if (empty) empty.hidden = visibleItems().length !== 0;
  }
  function filterItems() {
    var term = input.value.trim().toLowerCase();
    allItems().forEach(function (item) {
      var haystack = (item.textContent + ' ' + (item.getAttribute('data-command-search') || '')).toLowerCase();
      item.hidden = !!term && haystack.indexOf(term) === -1;
      item.classList.remove('is-active');
    });
    updateEmpty();
  }
  function clearLiveResults() {
    liveItems = [];
    if (liveResults) liveResults.textContent = '';
  }
  function addLiveItem(category, label, detail, href, value) {
    if (!liveResults) return;
    var item = document.createElement('a');
    item.className = 'command-item command-live-item';
    item.href = href;
    item.tabIndex = 0;
    item.setAttribute('data-command-search', category + ' ' + label + ' ' + detail);
    var focusTypes = { Entity: 'Object', Annotation: 'Object', Case: 'Case', Signal: 'Signal', Decision: 'Decision' };
    var focusId = value && value.id;
    var focusType = focusTypes[category];
    if (category === 'Saved view' && value && value.filter && Array.isArray(value.filter.object_ids) && value.filter.object_ids.length === 1) {
      focusId = value.filter.object_ids[0];
      focusType = 'Object';
    }
    if (focusType && focusId) {
      item.setAttribute('data-investigation-route', 'true');
      item.setAttribute('data-investigation-context', focusId);
      item.setAttribute('data-investigation-label', label);
      item.setAttribute('data-investigation-type', focusType);
      item.setAttribute('data-investigation-href', href);
    }
    item.innerHTML = '<span class="command-item-copy"><strong></strong><small></small></span><span class="command-live-type"></span>';
    item.querySelector('strong').textContent = label;
    item.querySelector('small').textContent = detail;
    item.querySelector('.command-live-type').textContent = category;
    liveResults.appendChild(item);
    liveItems.push(item);
    bindItem(item);
  }
  function renderSearchResults(data, savedViews) {
    clearLiveResults();
    var append = function (category, values, label, detail) {
      (values || []).slice(0, 4).forEach(function (value) {
        var id = value.id;
        var title = label(value);
        if (!id || !title) return;
        addLiveItem(category, title, detail(value), hrefFor(category, id, value), value);
      });
    };
    append('Evidence', data.records, function (value) { return value.source_type + ' · ' + value.id; }, function (value) { return value.connector_id || 'Source record'; });
    append('Connector', data.sensors, function (value) { return value.name; }, function (value) { return value.connector_type + (value.enabled ? ' · enabled' : ' · disabled'); });
    append('Identity', data.identities, function (value) { return value.username; }, function (value) { return value.role + (value.mfa_enabled ? ' · MFA enrolled' : ' · MFA missing'); });
    append('Entity', data.entities, function (value) { var p = value.properties || {}; return p.name || p.title || p.subject || p.id || value.id; }, function (value) { return 'Modeled object · ' + value.id; });
    append('Object type', data.object_types, function (value) { return value.name; }, function (value) { return 'Ontology contract · v' + value.version; });
    append('Case', data.incidents, function (value) { return value.title; }, function (value) { return value.severity + ' · ' + value.status; });
    append('Signal', data.events, function (value) { return value.event_type + ' · ' + value.group_key; }, function (value) { return value.status + ' · ' + value.id; });
    append('Action contract', data.action_types, function (value) { return value.name; }, function (value) { return 'Governed action · ' + value.id; });
    append('Decision', data.actions, function (value) { return value.action_type_id; }, function (value) { return value.outcome + ' · ' + value.id; });
    append('Template', data.templates, function (value) { return value.name; }, function (value) { return value.action_type + ' · v' + value.version; });
    append('Annotation', data.annotations, function (value) { return value.body; }, function (value) { return value.author + ' · Object 360 context · ' + value.object_id; });
    append('Audit', data.audits, function (value) { var entry = value.entry || value; return entry.change_type || entry.entity_type || 'Audit change'; }, function (value) { var entry = value.entry || value; return (value.service || 'audit') + ' · ' + (entry.entity_type || 'entity') + ' · ' + entry.entity_id; });
    append('Saved view', savedViews, function (value) { return value.name; }, function (value) { var filter = value.filter || {}; return (filter.surface || 'workspace') + ' · saved investigation'; });
    filterItems();
  }
  function hrefFor(category, id, value) {
    var encoded = encodeURIComponent(id);
    if (category === 'Evidence') return '/data/' + encoded;
    if (category === 'Connector') return '/sensors/' + encoded;
    if (category === 'Identity') return '/users/' + encoded;
    if (category === 'Entity') return '/ontology/objects/' + encoded + '/360';
    if (category === 'Object type') return '/ontology?type_id=' + encoded;
    if (category === 'Case') return '/incidents/' + encoded;
    if (category === 'Signal') return '/events/' + encoded;
    if (category === 'Decision') return '/actions/' + encoded;
    if (category === 'Action contract') return '/actions/library?q=' + encoded;
    if (category === 'Template') return '/action-templates';
    if (category === 'Annotation') return '/ontology/objects/' + encodeURIComponent(value.object_id) + '/360';
    if (category === 'Saved view') {
      var filter = (value && value.filter) || {};
      var objectIds = Array.isArray(filter.object_ids) ? filter.object_ids : [];
      if (objectIds.length === 1) return '/ontology/objects/' + encodeURIComponent(objectIds[0]) + '/360';
      if (objectIds.length >= 2) return '/ontology/compare?ids=' + encodeURIComponent(objectIds.slice(0, 6).join(','));
      var surface = String(filter.surface || 'overview');
      return surface === 'ontology' ? '/ontology' : '/' + surface;
    }
    var service = (value && value.service) || 'config-admin-service';
    var route = service === 'config-admin-service' ? 'config' : service === 'retention-service' ? 'retention' : service === 'auth-service' ? 'auth' : service === 'incident-service' ? 'incident' : service === 'ontology-service' ? 'ontology' : service === 'ingestion-gateway' ? 'ingestion' : service === 'egress-gateway' ? 'egress' : 'config';
    return '/audit-log/' + route + '/' + encoded;
  }
  function openPalette() {
    lastFocus = document.activeElement;
    palette.hidden = false;
    input.value = '';
    clearLiveResults();
    filterItems();
    input.focus();
  }
  function closePalette() {
    palette.hidden = true;
    if (lastFocus && typeof lastFocus.focus === 'function') lastFocus.focus();
  }
  launcher.addEventListener('click', openPalette);
  input.addEventListener('input', function () {
    filterItems();
    window.clearTimeout(searchTimer);
    var term = input.value.trim();
    if (term.length < 2) { clearLiveResults(); filterItems(); return; }
    var sequence = ++searchSequence;
    searchTimer = window.setTimeout(function () {
      fetch('/api/v1/search?q=' + encodeURIComponent(term) + '&scope=all', { credentials: 'same-origin', headers: { 'Accept': 'application/json' } })
        .then(function (response) { if (!response.ok) throw new Error('search unavailable'); return response.json(); })
        .then(function (data) {
          if (sequence !== searchSequence || input.value.trim() !== term) return;
          fetch('/api/v1/saved-views?q=' + encodeURIComponent(term), { credentials: 'same-origin', headers: { 'Accept': 'application/json' } })
            .then(function (response) { if (!response.ok) throw new Error('saved views unavailable'); return response.json(); })
            .then(function (savedViews) { if (sequence === searchSequence && input.value.trim() === term) renderSearchResults(data, savedViews); })
            .catch(function () { if (sequence === searchSequence && input.value.trim() === term) renderSearchResults(data, []); });
        })
        .catch(function () { if (sequence === searchSequence) { clearLiveResults(); filterItems(); } });
    }, 180);
  });
  palette.addEventListener('click', function (event) {
    if (event.target.hasAttribute('data-command-close')) closePalette();
  });
  input.addEventListener('keydown', function (event) {
    var visible = visibleItems();
    if (event.key === 'Escape') { event.preventDefault(); closePalette(); }
    if (event.key === 'ArrowDown' && visible.length) { event.preventDefault(); visible[0].focus(); }
  });
  function bindItem(item) {
    item.addEventListener('focus', function () {
      allItems().forEach(function (other) { other.classList.remove('is-active'); });
      item.classList.add('is-active');
    });
    item.addEventListener('keydown', function (event) {
      var visible = visibleItems();
      var index = visible.indexOf(item);
      if (event.key === 'Escape') { event.preventDefault(); closePalette(); }
      if (event.key === 'ArrowDown' && visible.length) { event.preventDefault(); visible[(index + 1) % visible.length].focus(); }
      if (event.key === 'ArrowUp' && visible.length) { event.preventDefault(); visible[(index <= 0 ? visible.length : index) - 1].focus(); }
    });
  }
  items.forEach(bindItem);
  document.addEventListener('keydown', function (event) {
    if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'k') { event.preventDefault(); openPalette(); }
    if (event.key === 'Escape' && !palette.hidden) closePalette();
    if (event.metaKey || event.ctrlKey || event.altKey || event.key === ' ') return;
    var target = event.target;
    if (target && (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.tagName === 'SELECT' || target.isContentEditable)) return;
    if (event.key.toLowerCase() === 'g') {
      event.preventDefault();
      window.clearTimeout(shortcutTimer);
      shortcutTimer = window.setTimeout(function () { shortcutTimer = null; }, 1200);
      return;
    }
    if (shortcutTimer) {
      var href = shortcutMap['g ' + event.key.toLowerCase()];
      window.clearTimeout(shortcutTimer);
      shortcutTimer = null;
      if (href) { event.preventDefault(); window.location.href = href; }
    }
  });
}());
