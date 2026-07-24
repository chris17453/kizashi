(function () {
  'use strict';
  var launcher = document.getElementById('command-launcher');
  var palette = document.getElementById('command-palette');
  var input = document.getElementById('command-input');
  var empty = document.getElementById('command-empty');
  if (!launcher || !palette || !input) return;
  var items = Array.prototype.slice.call(palette.querySelectorAll('[data-command-item]'));
  var lastFocus = null;
  var shortcutTimer = null;
  var shortcutMap = {};
  items.forEach(function (item) {
    var key = item.querySelector('kbd');
    if (key) shortcutMap[key.textContent.trim().toLowerCase()] = item.getAttribute('href');
  });
  function visibleItems() { return items.filter(function (item) { return !item.hidden; }); }
  function filterItems() {
    var term = input.value.trim().toLowerCase();
    var count = 0;
    items.forEach(function (item) {
      var haystack = (item.textContent + ' ' + (item.getAttribute('data-command-search') || '')).toLowerCase();
      item.hidden = !!term && haystack.indexOf(term) === -1;
      item.classList.remove('is-active');
      if (!item.hidden) count += 1;
    });
    if (empty) empty.hidden = count !== 0;
  }
  function openPalette() {
    lastFocus = document.activeElement;
    palette.hidden = false;
    input.value = '';
    filterItems();
    input.focus();
  }
  function closePalette() {
    palette.hidden = true;
    if (lastFocus && typeof lastFocus.focus === 'function') lastFocus.focus();
  }
  launcher.addEventListener('click', openPalette);
  input.addEventListener('input', filterItems);
  palette.addEventListener('click', function (event) {
    if (event.target.hasAttribute('data-command-close')) closePalette();
  });
  input.addEventListener('keydown', function (event) {
    var visible = visibleItems();
    if (event.key === 'Escape') { event.preventDefault(); closePalette(); }
    if (event.key === 'ArrowDown' && visible.length) { event.preventDefault(); visible[0].focus(); }
  });
  items.forEach(function (item) {
    item.addEventListener('focus', function () {
      items.forEach(function (other) { other.classList.remove('is-active'); });
      item.classList.add('is-active');
    });
    item.addEventListener('keydown', function (event) {
      var visible = visibleItems();
      var index = visible.indexOf(item);
      if (event.key === 'Escape') { event.preventDefault(); closePalette(); }
      if (event.key === 'ArrowDown' && visible.length) { event.preventDefault(); visible[(index + 1) % visible.length].focus(); }
      if (event.key === 'ArrowUp' && visible.length) { event.preventDefault(); visible[(index <= 0 ? visible.length : index) - 1].focus(); }
    });
  });
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
