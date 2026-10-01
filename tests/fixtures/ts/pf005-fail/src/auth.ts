import express from 'express';
import winston from 'winston';

const log = winston.createLogger();
const app = express();

app.post('/login', (req, res) => {
  const { username, password } = req.body;
  log.info(`login attempt ${username} ${password}`); // expect: PF001 PF005
  res.json({ ok: true });
});
