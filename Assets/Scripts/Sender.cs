using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using UnityEngine.UI;
using UnityEngine.Networking;

public class Sender : MonoBehaviour
{
    public Text TextToSend;
    public Text TextReceived;
    public HostToggle theHT;
    public ClientToggle theCT;
    public byte[] buffer;
    public int sz;
    public byte error;
    public bool isServer;

    public void sendhi(int hstid, int cnid, int chanid)
    {
        NetworkTransport.Send(hstid, cnid, chanid, buffer, sz, out error);
    }

    public void ClickSendButton()
    {

    }
}
