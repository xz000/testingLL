using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using UnityEngine.UI;
using UnityEngine.Networking;
using System.IO;
using System.Text;
using System.Runtime.Serialization.Formatters.Binary;

public class NetWriter : MonoBehaviour
{
    public string netFileName;
    public string nettxtPath;
    FileStream netfs;
    StreamWriter netSWriter;
    public float netFrameLength = 1f;
    float netCurrentLength = 0;
    public int netFrameNum = 0;
    public float LocalFrameLength = 1f;
    float LocalCurrentLength = 0;
    public int LocalFrameNum = 0;
    public static List<string> L2S;
    public List<string> L2R;

    byte[] buffer2s = new byte[1024];
    bool isstarted = false;

    private void FixedUpdate()
    {

        if (!isstarted)
            return;
        netCurrentLength += Time.fixedDeltaTime;
        while (netCurrentLength >= netFrameLength)
        {
            PrintList(ref ClickCatcher.LS);
            netCurrentLength -= netFrameLength;
            netFrameNum++;
        }
    }

    private void Update()
    {
        if (!isstarted)
            return;
        LocalCurrentLength += Time.deltaTime;
        while (LocalCurrentLength >= LocalFrameLength)
        {
            BinaryFormatter formatter = new BinaryFormatter();
            buffer2s=
            //序列化数据并发送
            
            LocalCurrentLength -= LocalFrameLength;
            netFrameNum++;
        }
    }

    private void OnEnable()
    {
        netFileName = "/" + "net" + string.Format("{0:D2}{1:D2}{2:D2}{3:D2}{4:D2}{5:D2}", System.DateTime.Now.Year, System.DateTime.Now.Month, System.DateTime.Now.Day, System.DateTime.Now.Hour, System.DateTime.Now.Minute, System.DateTime.Now.Second) + ".txt";
        nettxtPath = Application.dataPath + netFileName;
        netfs = new FileStream(nettxtPath, FileMode.OpenOrCreate, FileAccess.ReadWrite);
        netSWriter = new StreamWriter(netfs);
        netSWriter.WriteLine("Started");
    }

    private void OnDisable()
    {
        netSWriter.WriteLine("Stoped");
        netSWriter.Close();
    }

    void PrintList(ref List<string> theLS)
    {
        while (theLS.Count != 0)
        {
            netSWriter.WriteLine(theLS[0]);
            //netSWriter.WriteLine(netFrameNum + theLS[0]);
            theLS.RemoveAt(0);
        }
    }
}
