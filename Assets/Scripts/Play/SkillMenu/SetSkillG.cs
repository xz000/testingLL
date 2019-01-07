using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using UnityEngine.UI;
///using Photon;

public class SetSkillG : MonoBehaviour
{
    public CatcherKeys ck;
    public Toggle G1;
    public Image IconG;
    GameObject Soldier;

    public void SetG()
    {
        if (Soldier == null)
            Soldier = gameObject.GetComponent<SkillsLink>().mySoldier;
        AllGOff();
        if (G1.isOn)
        {
            //Soldier.GetComponent<TestSkill01>().MyImageScript = IconG.GetComponent<CooldownImage>();
            Soldier.GetComponent<TestSkill01>().enabled = true;
            return;
        }
    }

    public void AllGOff()
    {
        IconG.GetComponent<CooldownImage>().enabled = false;
        Soldier.GetComponent<TestSkill01>().enabled = false;
        //Soldier.GetComponent<TestSkill01>().MyImageScript = null;
    }
}
