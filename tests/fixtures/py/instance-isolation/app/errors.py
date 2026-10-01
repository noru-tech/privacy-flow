class AppError(Exception):
    def __init__(self, code, detail):
        super().__init__(code)
        self.detail = detail
