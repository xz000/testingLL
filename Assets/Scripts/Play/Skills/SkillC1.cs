using System.Collections;
using System.Collections.Generic;
using UnityEngine;
///using Photon;

public class SkillC1 : MonoBehaviour
{
    public CooldownImage MyImageScript;
    public GameObject MyBoostObj;
    private float currentcooldown;
    public float cooldowntime = 10;
    public bool skillavaliable;
    public float maxtime = 5;

    // Use this for initialization
    void Start()
    {
        currentcooldown = cooldowntime;
    }

    // Update is called once per frame
    void GoSkillC1()
    {
        if (skillavaliable)
        {
            GetComponent<DoSkill>().singing = 0;
            Skill();
        }
    }

    private void FixedUpdate()
    {
        if (skillavaliable)
            return;
        if (currentcooldown >= cooldowntime)
        {
            skillavaliable = true;
        }
        else
        {
            currentcooldown += Time.fixedDeltaTime;
            //MyImageScript.IconFillAmount = currentcooldown / cooldowntime;
        }
    }

    void Skill()
    {
        GetComponent<DoSkill>().BeforeSkill();
        currentcooldown = 0;
        skillavaliable = false;
        MyBoostObj.GetComponent<BoostScript>().sender = gameObject;
        GameObject MyBoost = Instantiate(MyBoostObj, transform.position, Quaternion.identity);
        MyBoost.layer = 2;
        GetComponent<HPScript>().booststart();
        MyBoostObj.GetComponent<BoostScript>().maxtime = maxtime;
        gameObject.GetComponent<DoSkill>().DoClearJob();
    }

    void SkillC1SetLevel(int i)
    {
        if (i == 0)
            enabled = false;
        else
            enabled = true;
    }

    public float CalcFA()
    {
        return currentcooldown / cooldowntime;
    }
}
