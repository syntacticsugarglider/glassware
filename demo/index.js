const rust = import('./pkg');

rust
    .then(m => m.entry())
    .catch(console.error);
