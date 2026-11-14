-- 链式代理：代理可以设置一个前置代理（如本机 Clash 的 http://127.0.0.1:7890），
-- 启动环境/检测代理时先经前置代理再连到这个代理。为空表示直连。
ALTER TABLE proxies ADD COLUMN chain_proxy TEXT;
