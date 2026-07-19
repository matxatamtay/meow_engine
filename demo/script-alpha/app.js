window.executionOrder.push('external');
const externalStatus = document.querySelector('#status');
externalStatus.setAttribute('class', 'ready');
externalStatus.textContent = 'External script changed this node and triggered the final style pass.';
