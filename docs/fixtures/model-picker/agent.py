#!/usr/bin/env python3
"""Local deterministic stdio agent fixture; it never uses credentials or network."""
import json
import os
import sys
import time
import threading
import uuid
import fcntl
from contextlib import contextmanager

behavior = os.environ.get("TW_FAKE_BEHAVIOR", "normal")
audit_path = os.environ.get("TW_FAKE_AUDIT")
state_path = os.environ.get("TW_FAKE_STATE")
mode = "kimi" if "acp" in sys.argv or os.path.basename(sys.argv[0]).startswith("kimi") else "codex"

if "--version" in sys.argv:
    print("codex-cli 0.163.0")
    sys.exit(0)

def audit(method, session=None):
    if audit_path:
        with open(audit_path, "a", encoding="utf-8") as output:
            fcntl.flock(output, fcntl.LOCK_EX)
            output.write(json.dumps({"provider": mode, "method": method, "session": session}) + "\n")
            output.flush()
            fcntl.flock(output, fcntl.LOCK_UN)

@contextmanager
def state():
    if not state_path:
        yield {}
        return
    with open(state_path, "a+", encoding="utf-8") as stream:
        fcntl.flock(stream, fcntl.LOCK_EX)
        stream.seek(0)
        try:
            value = json.load(stream)
        except (ValueError, OSError):
            value = {}
        yield value
        stream.seek(0)
        stream.truncate()
        json.dump(value, stream)
        stream.flush()
        fcntl.flock(stream, fcntl.LOCK_UN)

output_lock = threading.Lock()
turns = {}
permissions = {}

def emit(obj):
    with output_lock:
        print(json.dumps(obj), flush=True)

def reply(request, result=None, error=None):
    obj = {"jsonrpc": "2.0", "id": request["id"]}
    obj["error" if error else "result"] = {"code": -32000, "message": error} if error else result
    emit(obj)

def run_turn(request, session, turn):
    stop = turns[session]
    count = 20 if behavior in ("long_stream", "permission") else 2
    for index in range(count):
        if stop.wait(0.08): break
        if mode == "kimi":
            emit({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":session,"update":{"sessionUpdate":"agent_message_chunk","messageId":"reply","content":{"type":"text","text":f"chunk {index} "}}}})
        else:
            emit({"method":"item/agentMessage/delta","params":{"threadId":session,"turnId":turn,"itemId":"reply","delta":f"chunk {index} "}})
        if index == 1 and behavior == "permission":
            permission_id = "approval-" + uuid.uuid4().hex
            answered = threading.Event()
            permissions[permission_id] = answered
            if mode == "kimi":
                emit({"jsonrpc":"2.0","id":permission_id,"method":"session/request_permission","params":{"sessionId":session,"toolCall":{"title":"Fixture permission"},"options":[{"optionId":"allow","name":"Allow once","kind":"allow_once"},{"optionId":"deny","name":"Decline","kind":"reject_once"}]}})
            else:
                emit({"id":permission_id,"method":"item/commandExecution/requestApproval","params":{"threadId":session,"turnId":turn,"command":"fixture command","availableDecisions":["accept","decline"]}})
            while not answered.wait(0.05) and not stop.is_set(): pass
    if mode == "kimi":
        reply(request,{"stopReason":"cancelled" if stop.is_set() else "end_turn"})
    else:
        emit({"method":"turn/completed","params":{"threadId":session,"turn":{"id":turn,"status":"interrupted" if stop.is_set() else "completed"}}})
    turns.pop(session,None)

def kimi_options(model="kimi-a"):
    thought = "low" if behavior == "model_effort_change" and model == "kimi-b" else "high"
    return [
        {"type":"select", "id":"model", "category":"model", "name":"Model", "currentValue":model,
         "options":[{"value":"kimi-a","name":"Kimi A"},{"value":"kimi-b","name":"Kimi B"}]},
        {"type":"select", "id":"thinking", "category":"thought_level", "name":"Thinking", "currentValue":thought,
         "options":[{"value":thought,"name":thought.title()}]},
        {"type":"select", "id":"mode", "category":"mode", "name":"Mode", "currentValue":"normal",
         "options":[{"value":"normal","name":"Normal"}]},
    ]

def codex_models():
    return [{"model":"codex-a","displayName":"Codex A","description":"Fixture model",
             "defaultReasoningEffort":"low","supportedReasoningEfforts":[{"reasoningEffort":"low","description":"Fixture"}],"hidden":False}]

for line in sys.stdin:
    try:
        request = json.loads(line)
    except ValueError:
        continue
    method = request.get("method", "")
    if not method and "id" in request:
        answered = permissions.pop(request["id"], None)
        if answered: answered.set()
        continue
    params = request.get("params") or {}
    session = params.get("sessionId") or params.get("threadId")
    audit(method, session)
    if method == "session/cancel" and session in turns:
        turns[session].set()
    if "id" not in request:
        continue
    if mode == "kimi":
        if method == "initialize":
            reply(request,{"protocolVersion":1,"agentInfo":{"name":"Kimi Code CLI","version":"2.2.0"},"agentCapabilities":{"loadSession":True,"sessionCapabilities":{"list":{}}}})
        elif method == "session/new":
            if behavior == "slow_new": time.sleep(0.35)
            session = "fixture-" + uuid.uuid4().hex
            with state() as saved:
                saved[session] = {"cwd":params.get("cwd"),"model":"kimi-a"}
            if behavior == "disconnect_new": os._exit(0)
            if behavior == "no_new_reply": continue
            if behavior == "open_failed_new": reply(request,error="fixture uncertain new failure")
            elif behavior == "missing_new_id": reply(request,{"configOptions":kimi_options()})
            else: reply(request,{"sessionId":session,"configOptions":"invalid" if behavior == "malformed_config" else kimi_options()})
        elif method == "session/list":
            with state() as saved:
                rows=[{"sessionId":key,"cwd":value.get("cwd"),"title":"Fixture conversation"} for key,value in saved.items() if key.startswith("fixture-")]
            reply(request,{"sessions":rows,"nextCursor":None})
        elif method == "session/load":
            with state() as saved: model=saved.get(session,{}).get("model","kimi-a")
            reply(request,{"configOptions":kimi_options(model)})
        elif method == "session/set_config_option":
            with state() as saved:
                if session in saved and params.get("configId")=="model": saved[session]["model"]=params.get("value")
                model=saved.get(session,{}).get("model","kimi-a")
            if behavior == "model_effort_change" and params.get("configId")=="model":
                pending=kimi_options("kimi-a")
                pending[0]["currentValue"]=model
                emit({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":session,"update":{"sessionUpdate":"config_option_update","configOptions":pending}}})
                time.sleep(0.15)
            reply(request,{"configOptions":kimi_options(model)})
        elif method == "session/close":
            reply(request,{})
        elif method == "session/delete":
            if behavior == "cleanup_failure": reply(request,error="fixture cleanup failure")
            else:
                with state() as saved: saved.pop(session,None)
                reply(request,{})
        elif method == "session/prompt":
            turns[session]=threading.Event()
            threading.Thread(target=run_turn,args=(request,session,""),daemon=True).start()
        else: reply(request,{})
    else:
        if method == "initialize": reply(request,{})
        elif method == "model/list":
            if behavior == "fresh_empty_pages": reply(request,{"data":[],"nextCursor":uuid.uuid4().hex})
            else: reply(request,{"data":codex_models(),"nextCursor":None})
        elif method in ("project/create","project/read","project/update"):
            reply(request,{"project":{"id":params.get("projectId") or "fixture-project","name":"Fixture"}})
        elif method == "thread/list":
            with state() as saved:
                rows=[{"id":key,"projectId":value.get("project_id"),"preview":"Fixture conversation"} for key,value in saved.items() if value.get("kind")=="codex-thread" and value.get("project_id")==params.get("projectId")]
            reply(request,{"data":rows,"nextCursor":None})
        elif method in ("thread/start","thread/resume"):
            thread=params.get("threadId") or ("fixture-thread-"+uuid.uuid4().hex)
            project=params.get("projectId") or "fixture-project"
            if method == "thread/start":
                with state() as saved: saved[thread]={"kind":"codex-thread","project_id":project}
            reply(request,{"thread":{"id":thread,"projectId":project},"model":"codex-a","reasoningEffort":"low","approvalPolicy":"on-request","approvalsReviewer":"user","sandbox":{"type":"readOnly","networkAccess":False},"cwd":params.get("cwd")})
        elif method == "skills/list": reply(request,{"data":[]})
        elif method == "mcpServerStatus/list": reply(request,{"data":[],"nextCursor":None})
        elif method == "thread/goal/get": reply(request,{"goal":None})
        elif method == "thread/turns/list": reply(request,{"data":[],"nextCursor":None})
        elif method == "thread/read": reply(request,{"thread":{"id":session,"turns":[]},"data":[],"nextCursor":None})
        elif method == "turn/start":
            turn="fixture-turn-"+uuid.uuid4().hex
            turns[session]=threading.Event()
            reply(request,{"turn":{"id":turn}})
            threading.Thread(target=run_turn,args=(request,session,turn),daemon=True).start()
        elif method == "turn/interrupt":
            if session in turns: turns[session].set()
            reply(request,{})
        else: reply(request,{})
