using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using UnityEngine.UI;
using UnityEngine.Networking;

public class ClientToggle : MonoBehaviour
{
    public GameObject SRer;
    public Sender SenderScript;
    public Toggle CT;
    public InputField Cinput;
    public Text Ctext;
    public Text Stext;
    public Text CLabel;
    public int SelfNO;
    public int TargetNO;
    public int CChannelID;
    public int CntID;
    int hostId;
    public byte error;
    ConnectionConfig CCcFIG;
    HostTopology CosTT;

    public void ClickCT()
    {
        if (CT.isOn)
            Checkstart();
        else
            Cstop();
    }

    void Checkstart()
    {
        Cinput.interactable = false;
        int.TryParse(Stext.text, out SelfNO);
        int.TryParse(Ctext.text, out TargetNO);
        if (TargetNO <= 0 || SelfNO <= 0)
        {
            CT.isOn = false;
            return;
        }
        Cstart();
    }

    void Cstart()
    {
        SenderScript.isServer = false;
        NetworkTransport.Init();
        SenderScript.StartSelf();
        CCcFIG = new ConnectionConfig();
        CChannelID = CCcFIG.AddChannel(QosType.Reliable);
        CosTT = new HostTopology(CCcFIG, 10);
        hostId = NetworkTransport.AddHost(CosTT, SelfNO);
        CntID = NetworkTransport.Connect(hostId, "127.0.0.1", TargetNO, 0, out error);
        CLabel.text = "From " + SelfNO.ToString() + " to " + TargetNO.ToString();

        SenderScript.HTo = CosTT;
        SenderScript.HSID = hostId;
        SenderScript.CNID = CntID;
        SenderScript.CHANID = CChannelID;

        SenderScript.SendButton.SetActive(true);
    }

    void Cstop()
    {
        Cinput.interactable = true;
        NetworkTransport.Disconnect(hostId, CntID, out error);
        NetworkTransport.Shutdown();
        CLabel.text = "Stoped";
        SenderScript.ResetSelf();
    }
}
