import requests
from tests.conftest import DEFAULT_API_TOKEN, check_socket
from waiting import wait
import websocket

LOCALHOST = "127.0.0.1"


def test_hoprd_apis_should_be_available(setup_7_nodes):
    secure_api_port = setup_7_nodes["1"]["api_port"]
    wait(
        lambda: check_socket(LOCALHOST, secure_api_port),
        timeout_seconds=1,
        waiting_for=f"API port {secure_api_port} to become available",
    )

    insecure_api_port = setup_7_nodes["2"]["api_port"]
    wait(
        lambda: check_socket(LOCALHOST, insecure_api_port),
        timeout_seconds=1,
        waiting_for=f"API port {insecure_api_port} to become available",
    )


def test_hoprd_api_should_reject_connection_without_any_auth(setup_7_nodes):
    url = f"http://{LOCALHOST}:{setup_7_nodes['1']['api_port']}/api/v2/node/version"
    r = requests.get(url)

    assert r.status_code == 401


def test_hoprd_api_should_reject_connection_with_invalid_token(setup_7_nodes):
    bad_token = DEFAULT_API_TOKEN + "bad_content"

    url = f"http://{LOCALHOST}:{setup_7_nodes['1']['api_port']}/api/v2/node/version"
    headers = {"X-Auth-Token": f"{bad_token}"}

    r = requests.get(url, headers=headers)

    assert r.status_code == 401


def test_hoprd_api_should_accept_connection_with_valid_token(setup_7_nodes):
    url = f"http://{LOCALHOST}:{setup_7_nodes['1']['api_port']}/api/v2/node/version"
    headers = {"X-Auth-Token": f"{DEFAULT_API_TOKEN}"}

    r = requests.get(url, headers=headers)

    assert r.status_code == 200


def test_hoprd_api_should_reject_connection_without_at_least_basic_auth(setup_7_nodes):
    session = requests.Session()
    url = f"http://{LOCALHOST}:{setup_7_nodes['1']['api_port']}/api/v2/node/version"

    r = session.get(url)

    assert r.status_code == 401


def test_hoprd_api_should_accept_connection_with_valid_basic_auth(setup_7_nodes):
    session = requests.Session()
    session.auth = (DEFAULT_API_TOKEN, "")

    url = f"http://{LOCALHOST}:{setup_7_nodes['1']['api_port']}/api/v2/node/version"

    r = session.get(url)

    assert r.status_code == 200


def test_hoprd_api_should_reject_connection_with_invalid_basic_auth(setup_7_nodes):
    bad_token = DEFAULT_API_TOKEN + "bad_content"

    session = requests.Session()
    session.auth = (bad_token, "")

    url = f"http://{LOCALHOST}:{setup_7_nodes['1']['api_port']}/api/v2/node/version"

    r = session.get(url)

    assert r.status_code == 401


def test_hoprd_websocket_api_should_accept_connection_with_valid_cookie(setup_7_nodes):
    ws = websocket.WebSocket()
    ws.connect(
        f"ws://{LOCALHOST}:{setup_7_nodes['1']['api_port']}/",
        cookie=f"X-Auth-Token={DEFAULT_API_TOKEN}",
    )

    ws.send("alice")
    resp = ws.recv()

    assert resp == ""


def test_hoprd_websocket_api_should_accept_connection_with_token_as_query_param(
    setup_7_nodes,
):
    ws = websocket.WebSocket()
    ws.connect(f"ws://{LOCALHOST}:{setup_7_nodes['1']['api_port']}/?apiToken={DEFAULT_API_TOKEN}")

    ws.send("alice")
    resp = ws.recv()

    assert resp == ""


def test_hoprd_websocket_noauth_api_should_accept_connection_without_a_token(
    setup_7_nodes,
):
    ws = websocket.WebSocket()
    ws.connect(f"ws://{LOCALHOST}:{setup_7_nodes['2']['api_port']}")

    ws.send("alice")
    resp = ws.recv()

    assert resp == ""


def test_hoprd_websocket_noauth_api_should_accept_connection_with_an_invalid_token(
    setup_7_nodes,
):
    ws = websocket.WebSocket()
    ws.connect(
        f"ws://{LOCALHOST}:{setup_7_nodes['2']['api_port']}/",
        cookie=f"X-Auth-Token={DEFAULT_API_TOKEN}bull$h1t",
    )

    ws.send("alice")
    resp = ws.recv()

    assert resp == ""


def test_hoprd_websocket_api_should_reject_connection_on_invalid_path(setup_7_nodes):
    ws = websocket.WebSocket()
    try:
        ws.connect(
            f"ws://{LOCALHOST}:{setup_7_nodes['1']['api_port']}/inVaLiD_paTh",
            cookie=f"X-Auth-Token={DEFAULT_API_TOKEN}",
        )

        ws.send("alice")
    except websocket.WebSocketBadStatusException as e:
        assert "404 Not Found" in str(e)
    else:
        assert False


def test_hoprd_websocket_api_should_reject_connection_without_cookie(setup_7_nodes):
    ws = websocket.WebSocket()
    try:
        ws.connect(f"ws://{LOCALHOST}:{setup_7_nodes['1']['api_port']}")
        ws.send("alice")
    except websocket.WebSocketBadStatusException as e:
        assert "401 Unauthorized" in str(e)
    else:
        assert False


def test_hoprd_websocket_api_should_reject_connection_with_invalid_token(setup_7_nodes):
    ws = websocket.WebSocket()
    try:
        ws.connect(
            f"ws://{LOCALHOST}:{setup_7_nodes['1']['api_port']}",
            cookie=f"X-Auth-Token={DEFAULT_API_TOKEN + 'random_b$_appendix'}",
        )

        ws.send("alice")
    except websocket.WebSocketBadStatusException as e:
        assert "401 Unauthorized" in str(e)
    else:
        assert False
