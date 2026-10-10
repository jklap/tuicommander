document.querySelector('#pick').onclick = () => chrome.runtime.sendMessage({kind: 'arm'});
chrome.runtime.sendMessage({kind: 'panel_loaded', url: location.href});
