type Options = { apiKey: string; endpoint: string };

export class Transport {
  private _options: Options;

  constructor(options: Options) {
    const { apiKey, endpoint } = options;
    this._options = { apiKey, endpoint };
  }

  async send(body: string) {
    console.log('sending to', this._options.endpoint);
    await fetch(this._options.endpoint, { method: 'POST', body, headers: { 'X-Api-Key': this._options.apiKey } });
  }
}
