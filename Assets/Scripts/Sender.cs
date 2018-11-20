using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using UnityEngine.UI;
using UnityEngine.Networking;

public class Sender : MonoBehaviour
{
    public InputField TextToSend;
    public Text TextReceived;
    public HostToggle theHT;
    public ClientToggle theCT;
    public Image SignalLight;
    public byte[] buffer;
    public int sz;
    public int rcsz;
    public byte error;
    public bool started = false;
    public bool isServer;
    public GameObject SendButton;
    public HostTopology HTo;
    public int HSID;
    public int CNID;
    public int CHANID;
    byte[] rcbuffer = new byte[1024];
    int rcbfsz = 1024;

    public void sendhi(int hstid, int cnid, int chanid)
    {
        NetworkTransport.Send(hstid, cnid, chanid, buffer, sz, out error);
    }

    private void Start()
    {
        ResetSelf();
    }

    public void ResetSelf()
    {
        started = false;
        SendButton.SetActive(false);
        SignalLight.color = Color.white;
    }

    public void StartSelf()
    {
        started = true;
        SignalLight.color = Color.yellow;
    }

    public void ClickSendButton()
    {
        if (isServer)
        {
            return;
        }
        else
        {
            buffer = System.Text.Encoding.Default.GetBytes(TextToSend.text);
            sz = buffer.Length;
            //sendhi(HSID, CNID, CHANID);
            TextToSend.text = null;
        }
    }

    private void FixedUpdate()
    {
        if (!started)
            return;
        SignalControl();
    }

    public void SignalControl()
    {
        NetworkEventType recData = NetworkTransport.Receive(out HSID, out CNID, out CHANID, rcbuffer, rcbfsz, out rcsz, out error);
        switch (recData)
        {
            case NetworkEventType.Nothing:
                break;
            case NetworkEventType.BroadcastEvent:
                break;
            case NetworkEventType.ConnectEvent:
                SignalLight.color = Color.green;
                break;
            case NetworkEventType.DataEvent:
                break;
            case NetworkEventType.DisconnectEvent:
                SignalLight.color = Color.red;
                break;
        }
    }
}
