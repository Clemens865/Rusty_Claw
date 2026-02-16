// Rusty Claw Control UI — entry point

import { route, start } from './router.js';
import { connect } from './gateway.js';
import { mount as dashboard } from './pages/dashboard.js';
import { mount as sessions } from './pages/sessions.js';
import { mount as chat } from './pages/chat.js';
import { mount as channels } from './pages/channels.js';
import { mount as config } from './pages/config.js';

route('/', dashboard);
route('/sessions', sessions);
route('/chat', chat);
route('/channels', channels);
route('/config', config);

connect();
start();
