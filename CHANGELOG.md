# Changelog

## [0.1.14](https://github.com/Aminmiri82/Conductor/compare/app-v0.1.13...app-v0.1.14) (2026-10-25)


### Features

* **agent:** improve isolated app verification workflow ([#32](https://github.com/Aminmiri82/Conductor/issues/32)) ([907d435](https://github.com/Aminmiri82/Conductor/commit/907d4352d232098fc02a95c625bc7f98c7d77e35))
* **collections:** export collections to Postman ([#34](https://github.com/Aminmiri82/Conductor/issues/34)) ([718c5e4](https://github.com/Aminmiri82/Conductor/commit/718c5e45adc903c3cdac3ae833e9e1a52c74c54c))
* **dev:** add distinct app icons and dev launch config ([#35](https://github.com/Aminmiri82/Conductor/issues/35)) ([713d505](https://github.com/Aminmiri82/Conductor/commit/713d50515521fec8413058f47540e8d87c2dfba3))
* **perf:** add profiling harness and enforce performance rules ([#31](https://github.com/Aminmiri82/Conductor/issues/31)) ([c7db99a](https://github.com/Aminmiri82/Conductor/commit/c7db99ac8d33dacf25c2f4c64e6a3e5f3e9c6af5))
* **perf:** measure long background idles ([#36](https://github.com/Aminmiri82/Conductor/issues/36)) ([f9edd2b](https://github.com/Aminmiri82/Conductor/commit/f9edd2b4c1c760a97420e89bf17f692b9e69bcbc))
* **perf:** run performance jobs on labeled PRs ([#33](https://github.com/Aminmiri82/Conductor/issues/33)) ([48b362d](https://github.com/Aminmiri82/Conductor/commit/48b362d0cf6c81fca03d1dbf42f590e2bf6e3c89))
* **settings:** add collection profiles and redesign settings UI ([dd927a0](https://github.com/Aminmiri82/Conductor/commit/dd927a09d4cc74841f6f7677b0328fdd9e791b03))


### Fixes

* build the About menu metadata only on macOS ([#28](https://github.com/Aminmiri82/Conductor/issues/28)) ([ad5059e](https://github.com/Aminmiri82/Conductor/commit/ad5059ed5e6691cc52534ec04ed2fdfc27b6d0b0))
* **deps:** update TLS, SQLite and build dependencies for security fixes ([#29](https://github.com/Aminmiri82/Conductor/issues/29)) ([d21bf9f](https://github.com/Aminmiri82/Conductor/commit/d21bf9f6988daebeee616a68bc635faed821a551))
* **settings:** improve variables table layout and controls ([6eaae26](https://github.com/Aminmiri82/Conductor/commit/6eaae26c939237139c24962b3e347fa47a5116ba))
* **settings:** preserve selection and cancel edits safely ([28ad6b9](https://github.com/Aminmiri82/Conductor/commit/28ad6b9b6413c5ff72ec63ae1313d24b194e5ec3))

## [0.1.13](https://github.com/Aminmiri82/Conductor/compare/app-v0.1.12...app-v0.1.13) (2026-10-23)


### Features

* send QUERY requests, rename collections, and add New Folder (⌘⇧N) ([#24](https://github.com/Aminmiri82/Conductor/issues/24)) ([3698e5b](https://github.com/Aminmiri82/Conductor/commit/3698e5b1da000ac00be3b372ed387055044866e9))

## [0.1.12](https://github.com/Aminmiri82/Conductor/compare/app-v0.1.11...app-v0.1.12) (2026-10-23)


### Features

* manage collections in settings, rename folders, and speed up CI ([#22](https://github.com/Aminmiri82/Conductor/issues/22)) ([af70869](https://github.com/Aminmiri82/Conductor/commit/af708691ecdebda7013658ed06eaefe8e5423879))

## [0.1.11](https://github.com/Aminmiri82/Conductor/compare/app-v0.1.10...app-v0.1.11) (2026-10-23)


### Features

* create collections from scratch without a Postman import ([#20](https://github.com/Aminmiri82/Conductor/issues/20)) ([ac226b3](https://github.com/Aminmiri82/Conductor/commit/ac226b3c9eddc3256b4ee836704f2293742b2b90))

## [0.1.10](https://github.com/Aminmiri82/Conductor/compare/app-v0.1.9...app-v0.1.10) (2026-10-23)


### Features

* ship Windows and Linux installers with each release ([#17](https://github.com/Aminmiri82/Conductor/issues/17)) ([702f1b9](https://github.com/Aminmiri82/Conductor/commit/702f1b993570999840ec1c83b3ae663d09c584a8))


### Performance

* speed up response rendering and enforce size budgets in CI ([#16](https://github.com/Aminmiri82/Conductor/issues/16)) ([39e9c87](https://github.com/Aminmiri82/Conductor/commit/39e9c87a2a73212418d92f8e92a3a2ffb2170d9f))

## [0.1.9](https://github.com/Aminmiri82/Conductor/compare/app-v0.1.8...app-v0.1.9) (2026-10-18)


### Features

* **mac:** attach a DMG installer to each release ([#15](https://github.com/Aminmiri82/Conductor/issues/15)) ([abc6cfe](https://github.com/Aminmiri82/Conductor/commit/abc6cfe302bcb139972ad38bc63885fd958ac370))
* **variables:** scope environments to collections ([eca8e5a](https://github.com/Aminmiri82/Conductor/commit/eca8e5a57eb7bef4bd7cbf611a65acefcc6f33bf))

## [0.1.8](https://github.com/Aminmiri82/Conductor/compare/app-v0.1.7...app-v0.1.8) (2026-10-09)


### Features

* expand request workspace and collection management ([32db374](https://github.com/Aminmiri82/Conductor/commit/32db3740c6121a56b6acad0a1e578ff09b1b63c1))


### Maintenance

* **deps:** upgrade TypeScript to 7 ([d7694d0](https://github.com/Aminmiri82/Conductor/commit/d7694d015b8df3ed3066be0fd77de99d4d150132))
* **releasing:** add release workflow guidance ([c97065d](https://github.com/Aminmiri82/Conductor/commit/c97065df2a3f2a4de41cfb5a3c6a7556480e29d1))


### Testing

* add isolated coverage for database and request sends ([c436213](https://github.com/Aminmiri82/Conductor/commit/c436213ac7a64d515e2c88e1d10826184ca956a6))

## [0.1.7](https://github.com/Aminmiri82/Conductor/compare/app-v0.1.6...app-v0.1.7) (2026-10-08)


### Fixes

* **responses:** keep server key order and download raw response bodies ([#11](https://github.com/Aminmiri82/Conductor/issues/11)) ([46437c6](https://github.com/Aminmiri82/Conductor/commit/46437c6b32233ee633ac242afa5913db4c1794a6))

## [0.1.6](https://github.com/Aminmiri82/Conductor/compare/app-v0.1.5...app-v0.1.6) (2026-09-30)


### Automation

* preserve draft tags during title updates ([#9](https://github.com/Aminmiri82/Conductor/issues/9)) ([c1df0e0](https://github.com/Aminmiri82/Conductor/commit/c1df0e037c444b5e0fa07b8eeab631bbc7c63a18))

## [0.1.5](https://github.com/Aminmiri82/Conductor/compare/app-v0.1.4...app-v0.1.5) (2026-09-30)


### Fixes

* **ci:** use Conductor release titles ([#7](https://github.com/Aminmiri82/Conductor/issues/7)) ([ab0eb0e](https://github.com/Aminmiri82/Conductor/commit/ab0eb0e5bdbe7fac4774ba4e9bf130b1b046a535))

## [0.1.4](https://github.com/Aminmiri82/Conductor/compare/app-v0.1.3...app-v0.1.4) (2026-09-30)


### Fixes

* **ci:** resolve draft releases through the release list ([1580c4d](https://github.com/Aminmiri82/Conductor/commit/1580c4d74f73d71aaa582e448aaf4c93bc640112))

## [0.1.3](https://github.com/Aminmiri82/Conductor/compare/app-v0.1.2...app-v0.1.3) (2026-09-29)


### Features

* automate release PRs and isolate agent verification ([23448db](https://github.com/Aminmiri82/Conductor/commit/23448db251c4eba3cef7d4a7b30d03d4c29caa99))
