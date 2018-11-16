using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using UnityEngine.UI;
using UnityEngine.Networking;

public class HostToggle : MonoBehaviour {
    public Toggle HT;
    public InputField Hinput;
    public Text Htext;
    public Text SLabel;
    public int HostNO;
    public int HChannelID;
    ConnectionConfig CCFIG;
    HostTopology HOSTTT;

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
        NetworkTransport.Init();
        CCFIG = new ConnectionConfig();
        HChannelID = CCFIG.AddChannel(QosType.Reliable);
        HOSTTT = new HostTopology(CCFIG, 10);
        NetworkTransport.AddHost(HOSTTT, HostNO);
        SLabel.text = "Host on :" + HostNO.ToString();
    }

    void Hstop()
    {
        Hinput.interactable = true;
        NetworkTransport.Shutdown();
        SLabel.text = "Host Stoped";
    }
}
