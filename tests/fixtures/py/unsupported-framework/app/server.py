from aiohttp import web  # expect: PFC01


async def handle(request):
    data = await request.json()
    print(data)
    return web.json_response({"ok": True})
