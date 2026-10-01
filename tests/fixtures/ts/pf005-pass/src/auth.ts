import express from 'express';
import winston from 'winston';
import bcrypt from 'bcrypt';

const log = winston.createLogger();
const app = express();

app.post('/register', async (req, res) => {
  const hash = await bcrypt.hash(req.body.password, 12);
  log.info('stored a password hash', { length: hash.length });
  res.json({ ok: true });
});
