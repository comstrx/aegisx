"""Exercise embedded UI with real proxy traffic, actions and failure states."""
import json
import os
import sys
import threading
from pathlib import Path
from playwright.sync_api import sync_playwright

root=Path(__file__).resolve().parents[2]
os.environ.setdefault("AEGISX_BIN",str(root/"dist/aegisx"))
sys.path.insert(0,str(root/"server/tests"))
from integration_support import Fixture,TOKEN
fixture=Fixture()
fixture.setUpClass()
fixture.setUp()
errors=[]
stop=threading.Event()
checks=[]
print("Launching embedded browser QA",flush=True)
try:
    with fixture.proxy('set_model("observe")\nset_queue {capacity=256,timeout_ms=1000}\nset_cache {responses=true,decisions=true,deny_ttl_ms=60000}\nadd_route {name="restricted",path="/private",deny=true}',control=True,store=True) as proxy,sync_playwright() as automation:
        def traffic():
            index=0
            while not stop.is_set():
                try: fixture.fetch(proxy.port,"/private" if index%9==0 else "/products?item="+str(index%5),headers={"Cache-Control":"no-cache"} if index%7==0 else {})
                except OSError: return
                index+=1
                stop.wait(.03 if index%80<40 else .10)
        worker=threading.Thread(target=traffic,daemon=True)
        worker.start()
        browser=automation.chromium.launch(headless=True,**({"executable_path":os.environ["CHROMIUM_PATH"]} if "CHROMIUM_PATH" in os.environ else {}))
        page=browser.new_page(viewport={"width":1440,"height":1050},device_scale_factor=1)
        page.on("pageerror",lambda error:errors.append(str(error)))
        page.goto(f"http://localhost:{fixture.admin}",wait_until="networkidle")
        page.screenshot(path=str(root/"tmp/panel-v09-connect.png"),full_page=True,animations="disabled")
        page.get_by_label("Access token").fill(TOKEN)
        page.get_by_role("button",name="Connect",exact=True).click()
        page.get_by_role("heading",name="Upstream health").wait_for()
        page.wait_for_function("document.querySelectorAll('.row-button').length > 3")
        page.wait_for_timeout(12000)
        assert page.locator(".sidebar").evaluate("(element)=>getComputedStyle(element).backgroundColor") == "rgb(255, 255, 255)"
        assert float(page.locator(".stat-card").first.evaluate("(element)=>parseFloat(getComputedStyle(element).borderRadius)")) >= 18
        page.get_by_role("heading",name="Traffic waiting room",exact=True).wait_for()
        page.screenshot(path=str(root/"tmp/panel-v09-light.png"),full_page=True,animations="disabled")
        page.screenshot(path=str(root/"tmp/panel-v09-overview.png"),animations="disabled")
        page.get_by_role("button",name="Switch to dark theme").click()
        page.screenshot(path=str(root/"tmp/panel-v09-dark.png"),full_page=True,animations="disabled")
        page.get_by_role("button",name="Switch to light theme").click()
        checks.extend(["login","live charts","light and dark themes"])
        page.get_by_role("button",name="Pause live",exact=True).click()
        page.wait_for_timeout(1500)
        before=page.locator(".stat-card-value").first.inner_text()
        page.wait_for_timeout(1500)
        assert before==page.locator(".stat-card-value").first.inner_text()
        page.get_by_role("button",name="Resume live",exact=True).click()
        checks.append("pause and resume polling")
        page.get_by_role("button",name="Clear caches").click()
        page.get_by_role("status").filter(has_text="Stored cache entries cleared").wait_for()
        page.get_by_label("Search requests").fill("no-such-request")
        page.get_by_text("No matching requests",exact=True).wait_for()
        page.get_by_label("Search requests").fill("default")
        page.locator(".row-button").first.click()
        page.get_by_role("heading",name="Execution timeline").wait_for()
        assert "request_id" in page.locator(".detail").inner_text()
        page.get_by_role("button",name="Block future requests").click()
        page.get_by_label("Restriction reason").fill("Operator browser test")
        page.get_by_label("Restriction duration").fill("45")
        page.get_by_role("button",name="Apply restriction",exact=True).click()
        page.get_by_role("dialog",name="Restrict future admissions").wait_for(state="hidden")
        assert fixture.fetch(proxy.port,"/products")[0]==403
        page.get_by_role("button",name="Close details").click()
        page.get_by_role("button",name="Decisions",exact=True).click()
        page.get_by_role("button",name="Revoke",exact=True).first.click(timeout=5000)
        page.get_by_role("button",name="Revoke decision",exact=True).click()
        page.get_by_role("status").filter(has_text="Decision revoked").wait_for()
        assert fixture.fetch(proxy.port,"/products")[0]==200
        page.get_by_role("heading",name="Cooperative cancellation",exact=True).wait_for()
        checks.extend(["cache purge","search empty state","journey drawer","durable block form","revoke form","backend status verification"])
        page.get_by_role("button",name="Analysis",exact=True).click()
        page.get_by_role("heading",name="Background analysis").wait_for()
        page.get_by_role("heading",name="Capture & durability",exact=True).wait_for()
        page.get_by_text("Research-only compact distilled model",exact=False).wait_for()
        checks.append("model evaluation limitation is visible")
        state=fixture.api("/state")[1]
        assert state["model"]["parameter_count"]==895178
        assert state["configuration"]["feature_count"]==296
        assert state["storage"]["healthy"] and state["storage"]["committed_batches"]>0
        assert state["analysis"]["journal_enabled"] and state["analysis"]["journal_failures"]==0
        page.get_by_role("button",name="Configuration",exact=True).click()
        page.get_by_role("heading",name="Effective runtime").wait_for()
        page.get_by_role("heading",name="Routing policies").wait_for()
        page.screenshot(path=str(root/"tmp/panel-v09-configuration.png"),full_page=True,animations="disabled")
        checks.extend(["model and journal state","effective configuration"])
        page.keyboard.press("/")
        page.get_by_label("Search requests").wait_for()
        assert page.get_by_label("Search requests").evaluate("(element)=>element===document.activeElement")
        page.get_by_label("Search requests").fill("default")
        page.locator(".row-button").first.click()
        page.keyboard.press("Escape")
        page.get_by_role("dialog",name="Request journey").wait_for(state="hidden")
        checks.append("keyboard search and dialog escape")
        page.get_by_role("button",name="Overview",exact=True).click()
        page.set_viewport_size({"width":390,"height":844})
        page.screenshot(path=str(root/"tmp/panel-v09-mobile.png"),full_page=True,animations="disabled")
        overflow=page.evaluate("""()=>({width:innerWidth,scroll:document.documentElement.scrollWidth,items:[...document.querySelectorAll('body *')].filter(e=>e.getBoundingClientRect().right>innerWidth+1&&!e.closest('.table-scroll')).map(e=>({tag:e.tagName,cls:e.className,right:e.getBoundingClientRect().right,width:e.getBoundingClientRect().width})).slice(0,30)})""")
        if overflow["scroll"]>overflow["width"]: print(json.dumps(overflow),flush=True)
        assert overflow["scroll"]<=overflow["width"]
        for width in (320,768,1024,1440):
            page.set_viewport_size({"width":width,"height":900})
            assert page.evaluate("document.documentElement.scrollWidth<=innerWidth"), width
        page.emulate_media(reduced_motion="reduce")
        assert page.locator(".stat-card").first.evaluate("(element)=>parseFloat(getComputedStyle(element).animationDuration)") <= .001
        page.set_viewport_size({"width":390,"height":844})
        checks.extend(["white light sidebar","rounded surfaces","live admission queue","320–1440px responsive layouts","reduced motion"])
        assert page.evaluate("localStorage.length === 0 && sessionStorage.length === 0")
        page.locator('button[aria-label="Disconnect"]:visible').click()
        page.get_by_label("Access token").wait_for()
        page.get_by_label("Access token").fill("invalid-token-"*4)
        page.get_by_role("button",name="Connect",exact=True).click()
        page.get_by_text("Connection interrupted",exact=True).wait_for()
        page.get_by_role("button",name="Reconnect",exact=True).click()
        page.get_by_label("Access token").wait_for()
        checks.extend(["mobile overflow","token isolation","disconnect","invalid token error and recovery"])
        assert not errors,errors
        browser.close()
finally:
    stop.set()
    fixture.tearDownClass()
(root/"tmp/panel-qa.json").write_text(json.dumps({"page_errors":errors,"checks":checks},indent=2))
print("Panel browser QA passed",flush=True)
