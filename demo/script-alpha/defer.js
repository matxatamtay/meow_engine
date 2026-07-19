window.executionOrder.push('defer');
const order = document.querySelector('#order');
order.textContent = 'Execution order: ' + window.executionOrder.join(' > ');
document.title = 'MeowEngine Script Alpha: ' + window.executionOrder.join(' > ');
