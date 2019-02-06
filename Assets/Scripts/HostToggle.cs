using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using UnityEngine.UI;
using UnityEngine.Networking;

public class HostToggle : MonoBehaviour
{
    public GameObject SRer;
    public Sender SenderScript;
    public Toggle HT;
    public InputField Hinput;
    public Text Htext;
    public Text SLabel;
    public int HostNO;
    public int HChannelID;
    public int hostId;
    public byte Herror;
    //ConnectionConfig CCFIG;
    //HostTopology HOSTTT;

    public void ClickHT()
    {
        if (HT.isOn)
            Checkstart();
        else
            Hstop();
    }

    void Checkstart()
    {
        Hinput.interactable = false;
        int.TryParse(Htext.text, out HostNO);
        if (HostNO <= 0)
        {
            HT.isOn = false;
            return;
        }
        Hstart();
    }

    void Hstart()
    {
        ///Sender.isServer = true;
        Sender.clientNum = 0;
        //NetworkTransport.Init();
        SenderScript.StartSelf();
        //CCFIG = new ConnectionConfig();
        //HChannelID = CCFIG.AddChannel(QosType.Reliable);
        //NetWriter.channelID = CCFIG.AddChannel(QosType.ReliableSequenced);
        //HOSTTT = new HostTopology(CCFIG, 10);
        //hostId = NetworkTransport.AddHost(HOSTTT, HostNO);
        SLabel.text = "Host on :" + HostNO.ToString();
        SenderScript.CHANID = HChannelID;
    }

    void Hstop()
    {
        Hinput.interactable = true;
        /*if (NetworkTransport.IsStarted)
        {
            NetworkTransport.DisconnectNetworkHost(hostId, out Herror);
            NetworkTransport.Shutdown();
        }*/
        SLabel.text = "Host Stoped";
        SenderScript.ResetSelf();
    }
}
