const express = require('express');
const app = express();

app.post('/signup', (req, res) => {
  console.log(req.body);
  res.sendStatus(204);
});
