import Koa from 'koa'; // expect: PFC01

const app = new Koa();

app.use(async (ctx) => {
  console.log(ctx.request.body);
});
