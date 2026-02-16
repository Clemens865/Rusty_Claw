// Hash-based SPA router

const routes = new Map();
let currentCleanup = null;

export function route(path, handler) {
  routes.set(path, handler);
}

export function start() {
  window.addEventListener('hashchange', navigate);
  navigate();
}

function navigate() {
  const hash = window.location.hash.slice(1) || '/';
  const app = document.getElementById('app');

  // Run cleanup for previous page
  if (currentCleanup) {
    currentCleanup();
    currentCleanup = null;
  }

  // Find matching handler
  const handler = routes.get(hash);
  if (handler) {
    app.innerHTML = '';
    currentCleanup = handler(app) || null;
  } else {
    // Default to dashboard
    window.location.hash = '#/';
    return;
  }

  // Update active nav link
  document.querySelectorAll('.nav-link').forEach(link => {
    const linkRoute = link.dataset.route;
    if (linkRoute === hash) {
      link.classList.add('active');
    } else {
      link.classList.remove('active');
    }
  });
}
