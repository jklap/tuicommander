const target = document.querySelector('#target');
if (target) Object.assign(target, {__vueParentComponent: {type: {__file: 'fixture/app.ts'}}});
if (import.meta.hot) import.meta.hot.accept();
// S0 HMR measurement: file change after browser load.
