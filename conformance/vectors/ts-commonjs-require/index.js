const Mailer = require('./mailer');
const Emails = require('./emails');

module.exports = new Emails({ mailer: new Mailer() });
