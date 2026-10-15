import express from 'express';
import { track } from '@shop/shared';

const app = express();
app.use(express.json());

app.post('/signup', (req, res) => {
  const { email } = req.body as { email: string };
  console.log('signup', email); // expect: PF001
  track(email);
  res.sendStatus(201);
});

app.listen(4000);
