const express = require('express');
const app = express();

app.post('/users', handleCreate);

function handleCreate(req, res) {
  console.info('create user', req.body); // expect: PF001
  res.status(201).json({ ok: true });
}
