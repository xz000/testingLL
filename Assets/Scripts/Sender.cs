using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using UnityEngine.UI;
using UnityEngine.Networking;
using System;
using Bond;

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
    public static bool isServer;
    public static int clientNum;
    public GameObject SendButton;
    public NetWriter MyNS;
    public HostTopology HTo;
    public static int HSID;
    public static int CNID;
    public int CHANID;
    byte[] rcbuffer = new byte[1024];
    public int rcbfsz = 1024;
    public Toggle CCToggle;
    public CanvasGroup MCG;

    private void Start()
    {
        ResetSelf();
    }

    public void ResetSelf()
    {
        started = false;
        ResetS2();
    }

    void ResetS2()
    {
        SendButton.SetActive(false);
        SignalLight.color = Color.white;
        MyNS.isstarted = false;
        MyNS.enabled = false;
        CCToggle.isOn = false;
    }

    void ConnectDo()
    {
        SignalLight.color = Color.green;
        SendButton.SetActive(true);
        MyNS.enabled = true;//开启netwriter
        CCToggle.isOn = true;
        HideMC();
    }

    void DisconnectDo()
    {
        ResetS2();
        SignalLight.color = Color.red;
        ShowMC();
    }

    public void StartSelf()
    {
        started = true;
        SignalLight.color = Color.yellow;
    }

    public void ClickSendButton()
    {
        byte[] bff = new byte[1024];
        bff = System.Text.Encoding.Unicode.GetBytes(TextToSend.text);
        int bffsz = bff.Length;
        if (sz != 0)
        {
            NetworkTransport.Send(HSID, CNID, CHANID, bff, bffsz, out error);
            TextReceived.text = System.Text.Encoding.Unicode.GetString(buffer);
            TextToSend.text = null;
        }
    }

    public void ShowMC()
    {
        MCG.alpha = 1;
        MCG.blocksRaycasts = false;
        MCG.interactable = true;
    }

    public void HideMC()
    {
        MCG.interactable = false;
        MCG.blocksRaycasts = true;
        MCG.alpha = 0;
    }

    private void Update()
    {
        if (Input.GetKeyDown(KeyCode.Tab))
        {
            if (MCG.interactable)
                HideMC();
            else
                ShowMC();
        }
        if (!started)
            return;
        SignalControl();
    }

    public void SignalControl()
    {
        int RecChanID;
        NetworkEventType recData = NetworkTransport.Receive(out HSID, out CNID, out RecChanID, rcbuffer, rcbfsz, out rcsz, out error);
        switch (recData)
        {
            case NetworkEventType.Nothing:
                break;
            case NetworkEventType.BroadcastEvent:
                break;
            case NetworkEventType.ConnectEvent:
                ConnectDo();
                break;
            case NetworkEventType.DataEvent:
                if (RecChanID == CHANID)
                    PrintReceived();
                else
                    DeSerializeReceived();
                break;
            case NetworkEventType.DisconnectEvent:
                DisconnectDo();
                break;
        }
    }

    private void PrintReceived()
    {
        TextReceived.text = System.Text.Encoding.Unicode.GetString(rcbuffer);
        rcbuffer = new byte[1024];
    }

    void DeSerializeReceived()
    {
        //System.Array.Resize<byte>(ref rcbuffer, rcbfsz);
        MyNS.Eat(rcbuffer);
        rcbuffer = new byte[1024];
    }
}

[Serializable,Schema]
public class ClickData
{
    [Id(0)]
    public MButton blr;
    [Id(1)]
    public float xPos;
    [Id(2)]
    public float yPos;

    public void setdata(MButton b, float x, float y)
    {
        blr = b;
        xPos = x;
        yPos = y;
    }

    public string ToP()
    {
        string s2r = null;
        switch (blr)
        {
            case MButton.left:
                s2r = "Mouse0:" + "," + xPos.ToString() + "," + yPos.ToString();
                break;
            case MButton.right:
                s2r = "Mouse1:" + "," + xPos.ToString() + "," + yPos.ToString();
                break;
        }
        return s2r;
    }
}

[Serializable,Schema]
public class Data2S
{
    [Id(0)]
    public int frameNum;
    [Id(1)]
    public int clientNum;
    [Id(2)]
    public List<ClickData> clickDatas;
}

public enum MButton { left, right };
