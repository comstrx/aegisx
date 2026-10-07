import type {State} from "../lib/client";
import {Icon,Panel,format} from "./ui";

export function Queue({state}:{state:State}){
 const queue=state.queue,ratio=queue.capacity?Math.min(1,queue.waiting/queue.capacity):0;
 const completed=queue.completed;
 const average=completed?queue.total_us/completed/1000:0;
 return <Panel title="Traffic waiting room" description="Bounded waiting before forwarding. No request replay." className="queue-panel"
 action={<span className={"badge "+(queue.capacity?"success":"neutral")}><i/>{queue.capacity?"Enabled":"Disabled"}</span>}>
 <div className="queue-content"><div className="queue-gauge" role="img" aria-label={queue.waiting+" waiting requests out of "+queue.capacity+" slots"}>
 <svg viewBox="0 0 144 144" aria-hidden="true"><circle className="gauge-depth" cx="72" cy="75" r="55"/><circle className="gauge-track" cx="72" cy="72" r="55"/><circle className="gauge-fill" style={{opacity:ratio?1:0}} cx="72" cy="72" r="55" pathLength="100" strokeDasharray={ratio*100+" 100"}/></svg>
 <div><strong>{format(queue.waiting)}</strong><span>waiting now</span></div></div>
 <div className="queue-story"><span className="eyebrow">KEEP TRAFFIC MOVING</span><h3>{queue.waiting?"Making room for your requests.":queue.capacity?"Ready for the next traffic burst.":"Enable a waiting budget in Lua."}</h3>
 <p>{queue.capacity?format(queue.capacity)+" shared waiting slots · "+format(queue.timeout_ms)+" ms total wait budget.":"Add set_queue to wait for request, upstream, analysis or storage capacity."}</p>
 <div className="queue-flow"><span><Icon name="requests" size={14}/>Arrival</span><b>→</b><span className="current"><Icon name="clock" size={14}/>Wait for capacity</span><b>→</b><span><Icon name="upstream" size={14}/>Forward once</span></div></div>
 <div className="queue-metrics"><div><span>Waits resumed</span><strong>{format(queue.resumed)}</strong></div><div><span>Mean completed wait</span><strong>{format(average)}<small> ms</small></strong></div><div><span>Full / timed out</span><strong className={queue.full+queue.timed_out?"warning-text":""}>{format(queue.full)}<small> / </small>{format(queue.timed_out)}</strong></div></div></div>
 </Panel>;
}
